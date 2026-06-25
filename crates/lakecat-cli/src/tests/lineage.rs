use super::common::*;
use crate::*;

#[test]
fn qglake_lineage_drain_verifier_requires_delivered_events() {
    let verification = qglake_lineage_verification();
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 0,
            event_types: Vec::new(),
            graph_events: 0,
            lineage_events: 0,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![qglake_scan_planned_lineage_summary()],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject zero deliveries");
    assert!(
        err.to_string()
            .contains("qglake lineage drain delivered no outbox events")
    );

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 2,
            event_types: vec![
                "querygraph.bootstrap".to_string(),
                "table.scan-planned".to_string(),
            ],
            graph_events: 0,
            lineage_events: 0,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![qglake_scan_planned_lineage_summary()],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing lineage emissions");
    assert!(
        err.to_string()
            .contains("qglake lineage drain emitted no lineage events")
    );

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 2,
            event_types: vec![
                "querygraph.bootstrap".to_string(),
                "table.scan-planned".to_string(),
            ],
            graph_events: 0,
            lineage_events: 1,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![qglake_scan_planned_lineage_summary()],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing graph emissions");
    assert!(
        err.to_string()
            .contains("qglake lineage drain emitted no graph events")
    );

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["table.scan-planned".to_string()],
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
            events: vec![qglake_scan_planned_lineage_summary()],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require bootstrap replay");
    assert!(
        err.to_string()
            .contains("qglake lineage drain did not replay querygraph.bootstrap")
    );

    let mut drain = qglake_handoff_lineage_drain();
    let mut forged_summary = qglake_bootstrap_lineage_summary();
    forged_summary.event_type = "querygraph.bootstrap-shadow".to_string();
    drain.delivered += 1;
    drain.event_types.push("table.scan-planned".to_string());
    drain.graph_events += forged_summary.graph_events;
    drain.lineage_events += forged_summary.lineage_events;
    drain.events.push(forged_summary);
    let err = verify_qglake_lineage_drain(
        &drain,
        &qglake_handoff_lineage_verification(),
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject undeclared replay summaries");
    assert!(err.to_string().contains(
        "qglake lineage drain replay summary querygraph.bootstrap-shadow was not declared in event types"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
            graph_events: 1,
            lineage_events: 1,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: None,
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![qglake_scan_planned_lineage_summary()],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require read authorization proof");
    assert!(
        err.to_string().contains(
            "qglake lineage drain read is missing full SHA-256 authorization receipt hash"
        )
    );

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
            graph_events: 1,
            lineage_events: 1,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: None,
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![qglake_scan_planned_lineage_summary()],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require read request identity proof");
    assert!(
        err.to_string()
            .contains("qglake lineage drain read is missing request identity attestation state")
    );

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![qglake_scan_planned_lineage_summary()],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require bootstrap evidence");
    assert!(
        err.to_string()
            .contains("qglake lineage drain did not expose querygraph.bootstrap replay evidence")
    );

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: Vec::new(),
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require sink receipt evidence");
    assert!(
        err.to_string()
            .contains("querygraph.bootstrap replayEventHashes")
    );
    assert!(err.to_string().contains("full SHA-256"));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: None,
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require QueryGraph import replay hash");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence is missing full SHA-256 QueryGraph hashes"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("other-bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject mismatched replay hashes");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence does not match the accepted QueryGraph bundle"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:other".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject mismatched replay principal");
    assert!(err.to_string().contains(
        "qglake lineage drain replay principal did not match accepted principal did:example:agent"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("human".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject non-agent replay principals");
    assert!(err.to_string().contains(
        "qglake lineage drain replay principal kind did not match accepted principal kind agent"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: None,
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing authorization receipt proof");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence is missing full SHA-256 authorization receipt hash"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: None,
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing request identity state");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence is missing request identity attestation state"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: None,
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing agent delegation proof");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence is missing full SHA-256 agent delegation hash"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: None,
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing agent summary proof");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence is missing full SHA-256 agent summary signature hash"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 2,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject mismatched artifact counts");
    assert!(err.to_string().contains(
        "qglake lineage drain replay artifact counts do not match the accepted QueryGraph bundle"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 1,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: vec!["OpenLineage".to_string()],
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject mismatched replay standards");
    assert!(err.to_string().contains(
        "qglake lineage drain replay standards do not match the accepted QueryGraph bundle"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![LineageDrainEventSummary {
                event_id: "evt-bootstrap".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                catalog_config_defaults: Vec::new(),
                catalog_config_overrides: Vec::new(),
                catalog_config_endpoints: Vec::new(),
                principal_subject: Some("did:example:agent".to_string()),
                principal_kind: Some("agent".to_string()),
                authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                authorization_receipt_action: Some("graph-read".to_string()),
                request_identity_state: Some("verified".to_string()),
                request_identity_source: Some("x-lakecat-agent-did".to_string()),
                typedid_envelope_hash: None,
                typedid_proof_hash: None,
                agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                graph_events: 1,
                lineage_events: 1,
                bundle_hash: Some(qglake_fixture_hash("bundle")),
                graph_hash: Some(qglake_fixture_hash("graph")),
                open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                table_artifact_count: 1,
                view_artifact_count: 0,
                view_version_receipt_hashes: Vec::new(),
                view_version_receipt_chain_hashes: Vec::new(),
                view_version_receipt_chain_verified_count: 0,
                view_version_receipt_chains: Vec::new(),
                view_warehouse: None,
                view_namespace: Vec::new(),
                view_name: None,
                view_stable_id: None,
                view_version: None,
                expected_view_version: None,
                policy_binding_count: 0,
                policy_ids: Vec::new(),
                policy_id: None,
                policy_odrl_hash: None,
                project_count: None,
                project_ids: Vec::new(),
                server_count: None,
                server_ids: Vec::new(),
                storage_profile_count: None,
                storage_profile_ids: Vec::new(),
                storage_profile_id: None,
                storage_profile_provider: None,
                storage_profile_issuance_mode: None,
                storage_profile_location_prefix_hash: None,
                storage_profile_secret_ref_present: None,
                storage_profile_secret_ref_provider: None,
                storage_profile_secret_ref_hash: None,
                warehouse_count: None,
                warehouse_names: Vec::new(),
                table_commit_count: None,
                table_commit_sequence_numbers: Vec::new(),
                table_commit_hashes: Vec::new(),
                scan_task_count: None,
                file_scan_task_count: None,
                delete_file_count: None,
                child_plan_task_count: None,
                read_restriction: None,
                required_projection: Vec::new(),
                requested_projection: Vec::new(),
                effective_projection: Vec::new(),
                required_filters: Vec::new(),
                requested_stats_fields: Vec::new(),
                effective_stats_fields: Vec::new(),
                management_scope_project_id: None,
                management_scope_warehouse: None,
                standards: qglake_lineage_standards(),
                credential_count: None,
                credential_prefix_hashes: Vec::new(),
                credential_block_reason: None,
                raw_credential_exception_allowed: None,
                raw_credential_exception_reason: None,
                replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
            }],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject mismatched policy binding counts");
    assert!(err.to_string().contains(
        "qglake lineage drain replay policy binding count does not match the accepted QueryGraph bundle"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 2,
            event_types: vec![
                "querygraph.bootstrap".to_string(),
                "table.scan-planned".to_string(),
            ],
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
            events: vec![
                LineageDrainEventSummary {
                    event_id: "evt-bootstrap".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    catalog_config_defaults: Vec::new(),
                    catalog_config_overrides: Vec::new(),
                    catalog_config_endpoints: Vec::new(),
                    principal_subject: Some("did:example:agent".to_string()),
                    principal_kind: Some("agent".to_string()),
                    authorization_receipt_hash: Some(qglake_fixture_hash("authorization")),
                    authorization_receipt_action: Some("graph-read".to_string()),
                    request_identity_state: Some("verified".to_string()),
                    request_identity_source: Some("x-lakecat-agent-did".to_string()),
                    typedid_envelope_hash: None,
                    typedid_proof_hash: None,
                    agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
                    agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
                    graph_events: 0,
                    lineage_events: 0,
                    bundle_hash: Some(qglake_fixture_hash("bundle")),
                    graph_hash: Some(qglake_fixture_hash("graph")),
                    open_lineage_hash: Some(qglake_fixture_hash("openlineage")),
                    querygraph_import_hash: Some(qglake_fixture_hash("querygraph-import")),
                    table_artifact_count: 1,
                    view_artifact_count: 0,
                    view_version_receipt_hashes: Vec::new(),
                    view_version_receipt_chain_hashes: Vec::new(),
                    view_version_receipt_chain_verified_count: 0,
                    view_version_receipt_chains: Vec::new(),
                    view_warehouse: None,
                    view_namespace: Vec::new(),
                    view_name: None,
                    view_stable_id: None,
                    view_version: None,
                    expected_view_version: None,
                    policy_binding_count: 1,
                    policy_ids: Vec::new(),
                    policy_id: None,
                    policy_odrl_hash: None,
                    project_count: None,
                    project_ids: Vec::new(),
                    server_count: None,
                    server_ids: Vec::new(),
                    storage_profile_count: None,
                    storage_profile_ids: Vec::new(),
                    storage_profile_id: None,
                    storage_profile_provider: None,
                    storage_profile_issuance_mode: None,
                    storage_profile_location_prefix_hash: None,
                    storage_profile_secret_ref_present: None,
                    storage_profile_secret_ref_provider: None,
                    storage_profile_secret_ref_hash: None,
                    warehouse_count: None,
                    warehouse_names: Vec::new(),
                    table_commit_count: None,
                    table_commit_sequence_numbers: Vec::new(),
                    table_commit_hashes: Vec::new(),
                    scan_task_count: None,
                    file_scan_task_count: None,
                    delete_file_count: None,
                    child_plan_task_count: None,
                    read_restriction: None,
                    required_projection: Vec::new(),
                    requested_projection: Vec::new(),
                    effective_projection: Vec::new(),
                    required_filters: Vec::new(),
                    requested_stats_fields: Vec::new(),
                    effective_stats_fields: Vec::new(),
                    management_scope_project_id: None,
                    management_scope_warehouse: None,
                    standards: qglake_lineage_standards(),
                    credential_count: None,
                    credential_prefix_hashes: Vec::new(),
                    credential_block_reason: None,
                    raw_credential_exception_allowed: None,
                    raw_credential_exception_reason: None,
                    replay_event_hashes: vec![qglake_fixture_hash("replay-event")],
                    replay_open_lineage_hashes: vec![qglake_fixture_hash("replay-openlineage")],
                },
                qglake_scan_planned_lineage_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing bootstrap lineage projection");
    assert!(
        err.to_string()
            .contains("qglake lineage drain bootstrap replay emitted no lineage projection")
    );

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
            graph_events: 1,
            lineage_events: 1,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some("not-a-sha256-hash".to_string()),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![qglake_scan_planned_lineage_summary()],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject malformed read authorization hash");
    assert!(
        err.to_string().contains(
            "qglake lineage drain read is missing full SHA-256 authorization receipt hash"
        )
    );

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
            graph_events: 1,
            lineage_events: 1,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: Some(qglake_fixture_hash("typedid-proof")),
            events: vec![qglake_scan_planned_lineage_summary()],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject request TypeDID proof without envelope");
    assert!(
        err.to_string()
            .contains("qglake lineage drain read TypeDID proof hash requires an envelope hash")
    );

    let mut bootstrap_malformed_agent_hash = qglake_bootstrap_lineage_summary();
    bootstrap_malformed_agent_hash.graph_events = 1;
    bootstrap_malformed_agent_hash.agent_delegation_hash = Some("not-a-sha256-hash".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![bootstrap_malformed_agent_hash],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject malformed bootstrap agent delegation hash");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence is missing full SHA-256 agent delegation hash"
    ));

    let mut bootstrap_typedid_without_envelope = qglake_bootstrap_lineage_summary();
    bootstrap_typedid_without_envelope.graph_events = 1;
    bootstrap_typedid_without_envelope.typedid_proof_hash =
        Some(qglake_fixture_hash("typedid-proof"));
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 1,
            event_types: vec!["querygraph.bootstrap".to_string()],
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
            events: vec![bootstrap_typedid_without_envelope],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject bootstrap TypeDID proof without envelope");
    assert!(err.to_string().contains(
        "qglake lineage drain bootstrap replay TypeDID proof hash requires an envelope hash"
    ));

    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            qglake_human_credential_summary(),
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require restricted credential replay");
    let err = err.to_string();
    assert!(
        err.contains("qglake lineage drain did not replay the restricted credential probe"),
        "{err}"
    );

    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            qglake_restricted_credential_summary(),
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require trusted human credential replay");
    let err = err.to_string();
    assert!(
        err.contains("qglake lineage drain did not replay the trusted human credential probe"),
        "{err}"
    );

    let mut restricted_without_receipts = qglake_restricted_credential_summary();
    restricted_without_receipts.lineage_events = 0;
    restricted_without_receipts.replay_event_hashes.clear();
    restricted_without_receipts
        .replay_open_lineage_hashes
        .clear();
    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            restricted_without_receipts,
            qglake_human_credential_summary(),
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing restricted credential receipts");
    assert!(err.to_string().contains(
        "qglake lineage drain restricted credential replay emitted no lineage projection"
    ));

    let mut human_without_openlineage = qglake_human_credential_summary();
    human_without_openlineage.replay_open_lineage_hashes.clear();
    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            qglake_restricted_credential_summary(),
            human_without_openlineage,
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing trusted human credential receipts");
    assert!(err.to_string().contains(
        "qglake lineage drain trusted human credential replay is missing full SHA-256 sink receipt hashes"
    ));

    let mut human_without_prefix_hash = qglake_human_credential_summary();
    human_without_prefix_hash.credential_prefix_hashes.clear();
    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            qglake_restricted_credential_summary(),
            human_without_prefix_hash,
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing credential prefix hashes");
    assert!(
        err.to_string()
            .contains("trusted human credential replay credentialPrefixHashes count mismatch")
    );

    let mut human_with_duplicate_prefix_hash = qglake_human_credential_summary();
    let duplicate_hash = qglake_fixture_hash("duplicate-human-credential-prefix");
    human_with_duplicate_prefix_hash.credential_count = Some(2);
    human_with_duplicate_prefix_hash.credential_prefix_hashes =
        vec![duplicate_hash.clone(), duplicate_hash];
    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            qglake_restricted_credential_summary(),
            human_with_duplicate_prefix_hash,
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject duplicate credential prefix hashes");
    assert!(
        err.to_string()
            .contains("trusted human credential credentialPrefixHashes")
    );
    assert!(err.to_string().contains("duplicate-free"));

    let mut human_without_exception_reason = qglake_human_credential_summary();
    human_without_exception_reason.raw_credential_exception_reason = None;
    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            qglake_restricted_credential_summary(),
            human_without_exception_reason,
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing trusted human exception reason");
    assert!(err.to_string().contains(
        "qglake lineage drain trusted human credential replay did not prove audited standard credential vending"
    ));

    let mut restricted_with_exception_reason = qglake_restricted_credential_summary();
    restricted_with_exception_reason.raw_credential_exception_reason =
        Some("trusted-human-override".to_string());
    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            restricted_with_exception_reason,
            qglake_human_credential_summary(),
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject restricted raw exception reasons");
    assert!(err.to_string().contains(
        "qglake lineage drain restricted credential replay did not prove raw credentials were blocked"
    ));

    let mut human_without_ttl = qglake_human_credential_summary();
    human_without_ttl.read_restriction = None;
    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            qglake_restricted_credential_summary(),
            human_without_ttl,
            qglake_scan_planned_lineage_summary(),
            qglake_scan_tasks_fetched_lineage_summary(),
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing credential TTL evidence");
    assert!(
        err.to_string()
            .contains("trusted human credential replay is missing max credential TTL evidence")
    );

    let mut human_with_drifted_ttl = qglake_human_credential_summary();
    human_with_drifted_ttl.read_restriction = Some(json!({
        "allowed-columns": ["event_id", "occurred_at", "severity"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 60,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    }));
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                human_with_drifted_ttl,
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject credential TTL drift");
    assert!(
        err.to_string()
            .contains("trusted human credential replay TTL cap mismatch")
    );

    let mut human_without_purpose = qglake_human_credential_summary();
    human_without_purpose.read_restriction = Some(json!({
        "allowed-columns": ["event_id", "occurred_at", "severity"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    }));
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                human_without_purpose,
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing credential restriction purpose");
    assert!(err.to_string().contains(
        "qglake lineage drain trusted human credential read restriction is missing required field purpose"
    ));

    let mut human_with_drifted_purpose = qglake_human_credential_summary();
    human_with_drifted_purpose.read_restriction = Some(json!({
        "allowed-columns": ["event_id", "occurred_at", "severity"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "other-purpose",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    }));
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                human_with_drifted_purpose,
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject credential restriction purpose drift");
    assert!(
        err.to_string().contains(
            "qglake lineage drain restricted credential read restriction.purpose mismatch"
        )
    );

    let mut restricted_without_location_hash = qglake_restricted_credential_summary();
    restricted_without_location_hash.storage_profile_location_prefix_hash = None;
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                restricted_without_location_hash,
                qglake_human_credential_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject missing credential storage-scope hash");
    assert!(err.to_string().contains(
        "qglake lineage drain restricted credential replay is missing redacted storage-profile graph evidence"
    ));

    let mut restricted_missing_secret_ref_provider = qglake_restricted_credential_summary();
    restricted_missing_secret_ref_provider.storage_profile_secret_ref_present = Some(true);
    restricted_missing_secret_ref_provider.storage_profile_secret_ref_provider = None;
    restricted_missing_secret_ref_provider.storage_profile_secret_ref_hash =
        Some(qglake_fixture_hash("credential-secret-ref"));
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                restricted_missing_secret_ref_provider,
                qglake_human_credential_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err(
        "QGLake lineage drain should reject credential secret-ref presence without provider",
    );
    assert!(err.to_string().contains(
        "qglake lineage drain restricted credential replay is missing secret-ref provider evidence"
    ));

    let mut restricted_missing_secret_ref_hash = qglake_restricted_credential_summary();
    restricted_missing_secret_ref_hash.storage_profile_secret_ref_present = Some(true);
    restricted_missing_secret_ref_hash.storage_profile_secret_ref_provider =
        Some("vault".to_string());
    restricted_missing_secret_ref_hash.storage_profile_secret_ref_hash = None;
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                restricted_missing_secret_ref_hash,
                qglake_human_credential_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject credential secret-ref presence without hash");
    assert!(err.to_string().contains(
        "qglake lineage drain restricted credential replay is missing full SHA-256 secret-ref hash evidence"
    ));

    let mut restricted_short_secret_ref_hash = qglake_restricted_credential_summary();
    restricted_short_secret_ref_hash.storage_profile_secret_ref_present = Some(true);
    restricted_short_secret_ref_hash.storage_profile_secret_ref_provider =
        Some("vault".to_string());
    restricted_short_secret_ref_hash.storage_profile_secret_ref_hash =
        Some("sha256:credential-secret-ref".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                restricted_short_secret_ref_hash,
                qglake_human_credential_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject short credential secret-ref hashes");
    assert!(err.to_string().contains(
        "qglake lineage drain restricted credential replay is missing full SHA-256 secret-ref hash evidence"
    ));

    let mut restricted_hash_without_secret_ref = qglake_restricted_credential_summary();
    restricted_hash_without_secret_ref.storage_profile_secret_ref_hash =
        Some("sha256:credential-secret-ref".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                restricted_hash_without_secret_ref,
                qglake_human_credential_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject credential secret-ref hash without presence");
    assert!(err.to_string().contains(
        "qglake lineage drain restricted credential replay carried a secret-ref hash without secret-ref presence"
    ));

    let view_verification = qglake_view_lineage_verification();
    let mut bootstrap_with_view = qglake_bootstrap_lineage_summary();
    bootstrap_with_view.view_artifact_count = 1;
    bootstrap_with_view.view_version_receipt_hashes =
        vec![qglake_fixture_hash("view-version-receipt")];
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require accepted view replay");
    assert!(err.to_string().contains(
        "qglake lineage drain did not replay view evidence for lakecat:view:local:default:active_customers"
    ));

    let mut bootstrap_missing_view_receipt = bootstrap_with_view.clone();
    bootstrap_missing_view_receipt
        .view_version_receipt_hashes
        .clear();
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 9,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 3,
            lineage_events: 10,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_missing_view_receipt,
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require view version receipt hashes");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence is missing full SHA-256 view version receipt hashes"
    ));

    let mut bootstrap_malformed_view_receipt = bootstrap_with_view.clone();
    bootstrap_malformed_view_receipt.view_version_receipt_hashes =
        vec!["not-a-sha256-hash".to_string()];
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 9,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 3,
            lineage_events: 10,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_malformed_view_receipt,
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject malformed view version receipt hashes");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence is missing full SHA-256 view version receipt hashes"
    ));

    let mut bootstrap_drifted_view_receipt = bootstrap_with_view.clone();
    bootstrap_drifted_view_receipt.view_version_receipt_hashes =
        vec![qglake_fixture_hash("other-view-version-receipt")];
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 9,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 3,
            lineage_events: 10,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_drifted_view_receipt,
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject view receipt hash drift");
    assert!(err.to_string().contains(
        "qglake lineage drain replay evidence view version receipt hashes do not match the accepted QueryGraph bundle"
    ));

    let mut mismatched_view_replay = qglake_view_lineage_summary();
    mismatched_view_replay.view_version = Some(3);
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 9,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 3,
            lineage_events: 10,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                mismatched_view_replay,
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject mismatched view replay version");
    assert!(err.to_string().contains(
        "qglake lineage drain view replay for lakecat:view:local:default:active_customers did not preserve accepted view version 2"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 4,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 4,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require policy list replay");
    assert!(
        err.to_string()
            .contains("qglake lineage drain did not replay policy list evidence")
    );

    let mut policy_list_mismatch = qglake_policy_list_lineage_summary();
    policy_list_mismatch.policy_binding_count = 0;
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 5,
            event_types: vec![
                "querygraph.bootstrap".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
            ],
            graph_events: 2,
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
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                policy_list_mismatch,
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject mismatched policy list replay");
    assert!(err.to_string().contains(
        "qglake lineage drain policy list replay count does not match the accepted QueryGraph bundle"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 5,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
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
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require storage profile list replay");
    assert!(
        err.to_string()
            .contains("qglake lineage drain did not replay storage profile list evidence")
    );

    let mut empty_storage_profile_list = qglake_storage_profile_list_lineage_summary();
    empty_storage_profile_list.storage_profile_count = Some(0);
    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            qglake_bootstrap_lineage_summary(),
            qglake_restricted_credential_summary(),
            qglake_human_credential_summary(),
            qglake_policy_list_lineage_summary(),
            qglake_policy_upsert_lineage_summary(),
            empty_storage_profile_list,
        ]),
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject empty storage profile list replay");
    assert!(err.to_string().contains(
        "qglake lineage drain storage profile list replay did not expose any storage profiles"
    ));

    let mut malformed_storage_profile_upsert = qglake_storage_profile_upsert_lineage_summary();
    malformed_storage_profile_upsert.storage_profile_location_prefix_hash =
        Some("not-a-hash".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 7,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 7,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                malformed_storage_profile_upsert,
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject malformed storage-profile upsert hash");
    let err = err.to_string();
    assert!(
        err.contains(
            "qglake lineage drain storage profile upsert replay did not expose redacted credential-root evidence"
        ) || err.contains("storage-profile evidence does not match storage profile upsert replay"),
        "{err}"
    );

    let mut short_storage_profile_upsert = qglake_storage_profile_upsert_lineage_summary();
    short_storage_profile_upsert.storage_profile_location_prefix_hash =
        Some("sha256:storage-location-prefix".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 7,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 7,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                short_storage_profile_upsert,
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject short storage-profile upsert hash evidence");
    let err = err.to_string();
    assert!(
        err.contains(
            "qglake lineage drain storage profile upsert replay did not expose redacted credential-root evidence"
        ) || err.contains("storage-profile evidence does not match storage profile upsert replay"),
        "{err}"
    );

    let mut short_secret_ref_storage_profile_upsert =
        qglake_storage_profile_upsert_lineage_summary();
    short_secret_ref_storage_profile_upsert.storage_profile_secret_ref_present = Some(true);
    short_secret_ref_storage_profile_upsert.storage_profile_secret_ref_provider =
        Some("vault".to_string());
    short_secret_ref_storage_profile_upsert.storage_profile_secret_ref_hash =
        Some("sha256:storage-secret-ref".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 7,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 7,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                short_secret_ref_storage_profile_upsert,
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject short storage-profile secret-ref hashes");
    assert!(err.to_string().contains(
        "qglake lineage drain storage profile upsert replay is missing full SHA-256 secret-ref hash evidence"
    ));

    let mut contradictory_storage_profile_upsert = qglake_storage_profile_upsert_lineage_summary();
    contradictory_storage_profile_upsert.storage_profile_secret_ref_provider =
        Some("vault".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 7,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 7,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                contradictory_storage_profile_upsert,
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject contradictory secret-ref evidence");
    assert!(err.to_string().contains(
        "qglake lineage drain storage profile upsert replay carried secret-ref evidence without secret-ref presence"
    ));

    let mut restricted_profile_drift = qglake_restricted_credential_summary();
    restricted_profile_drift.storage_profile_id = Some("other-profile".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 7,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 7,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                restricted_profile_drift,
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject credential storage-profile drift");
    let err = err.to_string();
    assert!(
        err.contains(
            "qglake lineage drain restricted credential replay storage-profile evidence does not match storage profile upsert replay"
        ),
        "{err}"
    );

    let mut upsert_secret_ref_drift = qglake_storage_profile_upsert_lineage_summary();
    upsert_secret_ref_drift.storage_profile_secret_ref_present = Some(true);
    upsert_secret_ref_drift.storage_profile_secret_ref_provider = Some("vault".to_string());
    upsert_secret_ref_drift.storage_profile_secret_ref_hash =
        Some(qglake_fixture_hash("storage-secret-ref"));
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 7,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 7,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                upsert_secret_ref_drift,
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject credential secret-ref state drift");
    assert!(err.to_string().contains(
        "qglake lineage drain restricted credential replay storage-profile evidence does not match storage profile upsert replay"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 6,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
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
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require server list replay");
    assert!(
        err.to_string()
            .contains("qglake lineage drain did not replay server list evidence")
    );

    let mut server_list_without_graph = qglake_server_list_lineage_summary();
    server_list_without_graph.graph_events = 0;
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 10,
            event_types: vec![
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 1,
            lineage_events: 10,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                server_list_without_graph,
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require management graph projection");
    assert!(
        err.to_string().contains(
            "qglake lineage drain server list replay emitted no catalog graph projection"
        )
    );

    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            bootstrap_with_view.clone(),
            qglake_restricted_credential_summary(),
            qglake_human_credential_summary(),
            qglake_view_lineage_summary(),
            qglake_view_drop_lineage_summary(),
            qglake_policy_list_lineage_summary(),
            qglake_policy_upsert_lineage_summary(),
            qglake_storage_profile_list_lineage_summary(),
            qglake_storage_profile_upsert_lineage_summary(),
            qglake_server_list_lineage_summary(),
            qglake_project_list_lineage_summary(),
            qglake_warehouse_list_lineage_summary(),
        ]),
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err(
        "QGLake lineage drain should require tombstone receipt evidence for dropped accepted views",
    );
    let err = err.to_string();
    assert!(
        err.contains(
            "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers is missing full SHA-256 tombstone receipt evidence"
        ),
        "{err}"
    );

    let err = verify_qglake_lineage_drain(
        &qglake_lineage_drain_from_summaries(vec![
            bootstrap_with_view.clone(),
            qglake_restricted_credential_summary(),
            qglake_human_credential_summary(),
            qglake_view_lineage_summary(),
            qglake_view_drop_lineage_summary(),
            qglake_view_tombstone_receipt_lineage_summary(),
            qglake_policy_list_lineage_summary(),
            qglake_policy_upsert_lineage_summary(),
            qglake_storage_profile_list_lineage_summary(),
            qglake_storage_profile_upsert_lineage_summary(),
            qglake_server_list_lineage_summary(),
            qglake_project_list_lineage_summary(),
            qglake_warehouse_list_lineage_summary(),
        ]),
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require namespace receipt-chain evidence for dropped accepted views");
    assert!(err.to_string().contains(
        "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers is missing full SHA-256 namespace receipt-chain evidence"
    ));

    let mut drifted_receipt_chain = qglake_view_receipt_chain_lineage_summary();
    drifted_receipt_chain.view_namespace = vec!["other_namespace".to_string()];
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 13,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
                qglake_view_drop_lineage_summary(),
                qglake_view_tombstone_receipt_lineage_summary(),
                drifted_receipt_chain,
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject receipt-chain namespace drift");
    assert!(err.to_string().contains(
        "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers is missing full SHA-256 namespace receipt-chain evidence for the accepted view namespace"
    ));

    let mut receipt_chain_count_drift = qglake_view_receipt_chain_lineage_summary();
    receipt_chain_count_drift.view_version_receipt_chain_verified_count = 2;
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 12,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 13,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
                qglake_view_drop_lineage_summary(),
                qglake_view_tombstone_receipt_lineage_summary(),
                receipt_chain_count_drift,
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject receipt-chain count drift");
    assert!(err.to_string().contains(
        "qglake lineage drain namespace receipt-chain replay for lakecat:view:local:default:active_customers verified-chain count does not match chain hash evidence"
    ));

    let mut uncovered_tombstone_chain = qglake_view_receipt_chain_lineage_summary();
    uncovered_tombstone_chain.view_version_receipt_hashes =
        vec![qglake_fixture_hash("other-view-receipt")];
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 12,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 13,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
                qglake_view_drop_lineage_summary(),
                qglake_view_tombstone_receipt_lineage_summary(),
                uncovered_tombstone_chain,
                qglake_policy_list_lineage_summary(),
                qglake_policy_upsert_lineage_summary(),
                qglake_storage_profile_list_lineage_summary(),
                qglake_storage_profile_upsert_lineage_summary(),
                qglake_server_list_lineage_summary(),
                qglake_project_list_lineage_summary(),
                qglake_warehouse_list_lineage_summary(),
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err(
        "QGLake lineage drain should reject tombstone receipts outside the namespace chain",
    );
    assert!(err.to_string().contains(
        "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers tombstone receipt hashes are not covered by namespace receipt-chain evidence"
    ));

    let mut unguarded_drop_replay = qglake_view_drop_lineage_summary();
    unguarded_drop_replay.expected_view_version = None;
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 16,
            event_types: vec![
                "querygraph.bootstrap".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "view.dropped".to_string(),
                "view.version-receipts-listed".to_string(),
                "view.version-receipt-chains-listed".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
            ],
            graph_events: 16,
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
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
                unguarded_drop_replay,
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
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require guarded drop replay for accepted views");
    assert!(err.to_string().contains(
        "qglake lineage drain view drop replay for lakecat:view:local:default:active_customers did not preserve expected view version 2"
    ));

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 12,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require table commit history replay");
    assert!(
        err.to_string()
            .contains("qglake lineage drain did not replay table commit history evidence")
    );

    let mut commit_history_without_summary = qglake_table_commit_history_lineage_summary();
    commit_history_without_summary.table_commit_count = None;
    commit_history_without_summary
        .table_commit_sequence_numbers
        .clear();
    commit_history_without_summary.table_commit_hashes.clear();
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_without_summary,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require compact table commit history summary");
    assert!(err.to_string().contains(
        "qglake lineage drain table commit history replay is missing compact commit summary or SHA-256 commit hash evidence"
    ));

    let mut commit_history_without_principal = qglake_table_commit_history_lineage_summary();
    commit_history_without_principal.principal_subject = None;
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_without_principal,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require commit-history principal proof");
    assert!(err.to_string().contains(
        "qglake lineage drain table commit history replay is missing principal subject evidence"
    ));

    let mut commit_history_without_principal_kind = qglake_table_commit_history_lineage_summary();
    commit_history_without_principal_kind.principal_kind = None;
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_without_principal_kind,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require commit-history principal kind proof");
    assert!(err.to_string().contains(
        "qglake lineage drain table commit history replay is missing principal kind evidence"
    ));

    let mut commit_history_with_principal_drift = qglake_table_commit_history_lineage_summary();
    commit_history_with_principal_drift.principal_subject = Some("did:example:other".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_with_principal_drift,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject commit-history principal drift");
    assert!(err.to_string().contains(
        "qglake lineage drain table commit history replay principal did not match accepted principal did:example:agent"
    ));

    let mut commit_history_with_principal_kind_drift =
        qglake_table_commit_history_lineage_summary();
    commit_history_with_principal_kind_drift.principal_kind = Some("human".to_string());
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_with_principal_kind_drift,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject commit-history principal kind drift");
    assert!(err.to_string().contains(
        "qglake lineage drain table commit history replay principal kind did not match accepted principal kind agent"
    ));

    let mut commit_history_with_malformed_hash = qglake_table_commit_history_lineage_summary();
    commit_history_with_malformed_hash.table_commit_hashes = vec!["sha256:short".to_string()];
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_with_malformed_hash,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject short table commit hashes");
    assert!(err.to_string().contains(
        "qglake lineage drain table commit history replay is missing compact commit summary or SHA-256 commit hash evidence"
    ));

    let mut commit_history_with_count_drift = qglake_table_commit_history_lineage_summary();
    commit_history_with_count_drift.table_commit_count = Some(2);
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_with_count_drift,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject table commit count drift");
    assert!(err.to_string().contains(
        "qglake lineage drain table commit history replay count does not match sequence-number and commit-hash evidence"
    ));

    let mut commit_history_with_duplicate_sequence = qglake_table_commit_history_lineage_summary();
    commit_history_with_duplicate_sequence.table_commit_count = Some(2);
    commit_history_with_duplicate_sequence.table_commit_sequence_numbers = vec![1, 1];
    commit_history_with_duplicate_sequence.table_commit_hashes = vec![
        qglake_fixture_hash("table-commit-one"),
        qglake_fixture_hash("table-commit-two"),
    ];
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_with_duplicate_sequence,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject duplicate table commit sequences");
    assert!(err.to_string().contains(
        "qglake lineage drain table commit history replay sequence numbers must be positive and strictly increasing"
    ));

    let mut commit_history_with_duplicate_hash = qglake_table_commit_history_lineage_summary();
    commit_history_with_duplicate_hash.table_commit_count = Some(2);
    commit_history_with_duplicate_hash.table_commit_sequence_numbers = vec![1, 2];
    let duplicate_hash = qglake_fixture_hash("commit-history-duplicate");
    commit_history_with_duplicate_hash.table_commit_hashes =
        vec![duplicate_hash.clone(), duplicate_hash];
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 14,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_with_duplicate_hash,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject duplicate table commit hashes");
    assert!(err.to_string().contains(
        "qglake lineage drain table commit history replay commit hashes must not contain duplicates"
    ));

    let mut commit_history_without_graph = qglake_table_commit_history_lineage_summary();
    commit_history_without_graph.graph_events = 0;
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 14,
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
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 13,
            lineage_events: 15,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                commit_history_without_graph,
                qglake_scan_planned_lineage_summary(),
                qglake_scan_tasks_fetched_lineage_summary(),
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require table commit graph projection");
    assert!(
        err.to_string().contains(
            "qglake lineage drain table commit history replay emitted no graph projection"
        )
    );

    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 14,
            event_types: vec![
                "table.scan-planned".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "view.dropped".to_string(),
                "view.version-receipts-listed".to_string(),
                "view.version-receipt-chains-listed".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "querygraph.bootstrap".to_string(),
            ],
            graph_events: 4,
            lineage_events: 15,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should require fetched scan task replay");
    assert!(
        err.to_string()
            .contains("qglake lineage drain did not replay scan task fetch evidence")
    );

    let mut fetched_with_restriction_drift = qglake_scan_tasks_fetched_lineage_summary();
    fetched_with_restriction_drift.read_restriction = Some(json!({
        "allowed-columns": ["event_id", "occurred_at"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    }));
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 16,
            event_types: vec![
                "querygraph.bootstrap".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "view.dropped".to_string(),
                "view.version-receipts-listed".to_string(),
                "view.version-receipt-chains-listed".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
            ],
            graph_events: 16,
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
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                fetched_with_restriction_drift,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject scan restriction drift");
    assert!(
        err.to_string()
            .contains("qglake lineage drain scan planning read restriction")
    );
    assert!(err.to_string().contains("allowed-columns"));

    let mut fetched_with_filter_drift = qglake_scan_tasks_fetched_lineage_summary();
    fetched_with_filter_drift.required_filters = vec![json!({
        "type": "not-eq",
        "term": "severity",
        "value": "info"
    })];
    let err = verify_qglake_lineage_drain(
        &LineageDrainResponse {
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
                "policy-binding.upserted".to_string(),
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
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
                fetched_with_filter_drift,
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect_err("QGLake lineage drain should reject fetched filter drift");
    assert!(err.to_string().contains(
        "qglake lineage drain scan task fetch replay required filters do not exactly preserve fetched row predicate"
    ));

    verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 17,
            event_types: vec![
                "querygraph.bootstrap".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "view.dropped".to_string(),
                "view.version-receipts-listed".to_string(),
                "view.version-receipt-chains-listed".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
            ],
            graph_events: 17,
            lineage_events: 17,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                bootstrap_with_view.clone(),
                qglake_restricted_credential_summary(),
                qglake_human_credential_summary(),
                qglake_view_lineage_summary(),
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
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect("QGLake lineage drain should accept dropped view evidence with namespace receipt-chain evidence");

    verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 14,
            event_types: vec![
                "querygraph.bootstrap".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "view.upserted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
            ],
            graph_events: 15,
            lineage_events: 14,
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
            ],
        },
        &view_verification,
        Some("did:example:agent"),
        1,
    )
    .expect("QGLake lineage drain should accept replayed view evidence");

    verify_qglake_lineage_drain(
        &LineageDrainResponse {
            delivered: 13,
            event_types: vec![
                "querygraph.bootstrap".to_string(),
                "credentials.vend-attempted".to_string(),
                "credentials.vend-attempted".to_string(),
                "policy-binding.listed".to_string(),
                "policy-binding.upserted".to_string(),
                "storage-profile.listed".to_string(),
                "storage-profile.upserted".to_string(),
                "server.listed".to_string(),
                "project.listed".to_string(),
                "warehouse.listed".to_string(),
                "table.commits-listed".to_string(),
                "table.scan-planned".to_string(),
                "table.scan-tasks-fetched".to_string(),
            ],
            graph_events: 13,
            lineage_events: 13,
            principal_subject: Some("did:example:agent".to_string()),
            principal_kind: Some("agent".to_string()),
            authorization_receipt_hash: Some(qglake_fixture_hash("lineage-read")),
            authorization_receipt_action: Some("lineage-read".to_string()),
            request_identity_state: Some("verified".to_string()),
            request_identity_source: Some("x-lakecat-agent-did".to_string()),
            typedid_envelope_hash: None,
            typedid_proof_hash: None,
            events: vec![
                qglake_bootstrap_lineage_summary(),
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
            ],
        },
        &verification,
        Some("did:example:agent"),
        1,
    )
    .expect("QGLake lineage drain should accept delivered outbox events");
}
