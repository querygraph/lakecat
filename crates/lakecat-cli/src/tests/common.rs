use crate::*;

pub(crate) const QGLAKE_TEST_LOCATION: &str = "file:///tmp/lakecat-qglake/events";

pub(crate) fn qglake_add_management_receipt_hashes(management: &mut Value) {
    management["serverReplayEventHashes"] =
        json!([qglake_fixture_hash("server-list-replay-event")]);
    management["serverOpenLineageHashes"] = json!([qglake_fixture_hash("server-list-openlineage")]);
    management["projectReplayEventHashes"] =
        json!([qglake_fixture_hash("project-list-replay-event")]);
    management["projectOpenLineageHashes"] =
        json!([qglake_fixture_hash("project-list-openlineage")]);
    management["warehouseReplayEventHashes"] =
        json!([qglake_fixture_hash("warehouse-list-replay-event")]);
    management["warehouseOpenLineageHashes"] =
        json!([qglake_fixture_hash("warehouse-list-openlineage")]);
    management["policyReplayEventHashes"] =
        json!([qglake_fixture_hash("policy-list-replay-event")]);
    management["policyOpenLineageHashes"] = json!([qglake_fixture_hash("policy-list-openlineage")]);
    management["policyUpsertProof"] = json!({
        "policyId": "agent-columns",
        "odrlHash": qglake_fixture_hash("policy-odrl"),
        "principalSubject": "did:example:agent",
        "principalKind": "agent",
        "authorizationReceiptHash": qglake_fixture_hash("policy-upsert-authorization"),
        "authorizationReceiptAction": "policy-manage",
        "graphEvents": 1,
        "replayEventHashes": [qglake_fixture_hash("policy-upsert-replay")],
        "openLineageHashes": [qglake_fixture_hash("policy-upsert-openlineage")]
    });
    management["storageProfileReplayEventHashes"] =
        json!([qglake_fixture_hash("storage-profile-list-replay-event")]);
    management["storageProfileOpenLineageHashes"] =
        json!([qglake_fixture_hash("storage-profile-list-openlineage")]);
}

pub(crate) fn qglake_handoff_summary_json() -> Value {
    let mut summary = json!({
        "schemaVersion": "lakecat.qglake.handoff-summary.v1",
        "status": "verified",
        "catalogUrl": "http://127.0.0.1:18181",
        "principal": "did:example:agent",
        "warehouse": "local",
        "namespace": "default",
        "table": "events",
        "querygraphVerification": {
            "tableCount": 1,
            "viewCount": 1,
            "verifiedTables": [
                "lakecat:table:local:default:events"
            ],
            "verifiedViews": [
                "lakecat:view:local:default:active_customers_view"
            ],
            "bundleHash": qglake_fixture_hash("bundle"),
            "graphHash": qglake_fixture_hash("graph"),
            "openLineageHash": qglake_fixture_hash("openlineage"),
            "querygraphImportHash": qglake_fixture_hash("querygraph-import"),
            "standards": [
                "Iceberg REST",
                "Croissant",
                "CDIF",
                "OSI handoff",
                "ODRL",
                "Grust catalog graph",
                "OpenLineage"
            ]
        },
        "querygraphImportVerification": {
            "matchesVerify": true,
            "tableCount": 1,
            "viewCount": 1,
            "verifiedTables": [
                "lakecat:table:local:default:events"
            ],
            "verifiedViews": [
                "lakecat:view:local:default:active_customers_view"
            ],
            "bundleHash": qglake_fixture_hash("bundle"),
            "graphHash": qglake_fixture_hash("graph"),
            "openLineageHash": qglake_fixture_hash("openlineage"),
            "querygraphImportHash": qglake_fixture_hash("querygraph-import"),
            "standards": [
                "Iceberg REST",
                "Croissant",
                "CDIF",
                "OSI handoff",
                "ODRL",
                "Grust catalog graph",
                "OpenLineage"
            ]
        },
        "lakecatReplayVerification": {
            "schemaVersion": "lakecat.qglake.replay-verification.v1",
            "status": "verified",
            "matchesQueryGraph": true,
            "requestIdentityProof": {
                "principalSubject": "did:example:agent",
                "principalKind": "agent",
                "requestIdentitySource": "x-lakecat-agent-did",
                "requestIdentityState": "unverified",
                "authorizationReceiptHash": qglake_fixture_hash("identity"),
                "authorizationReceiptAction": "lineage-read",
                "typedidEnvelopeHash": null,
                "typedidProofHash": null
            },
            "queryGraphBootstrapProof": {
                "bundleHash": qglake_fixture_hash("bundle"),
                "graphHash": qglake_fixture_hash("graph"),
                "openLineageHash": qglake_fixture_hash("openlineage"),
                "queryGraphImportHash": qglake_fixture_hash("querygraph-import"),
                "tableArtifactCount": 1,
                "viewArtifactCount": 1,
                "policyBindingCount": 1,
                "standards": [
                    "Iceberg REST",
                    "Croissant",
                    "CDIF",
                    "OSI handoff",
                    "ODRL",
                    "Grust catalog graph",
                    "OpenLineage"
                ],
                "principalSubject": "did:example:agent",
                "principalKind": "agent",
                "requestIdentitySource": "x-lakecat-agent-did",
                "requestIdentityState": "unverified",
                "authorizationReceiptHash": qglake_fixture_hash("identity"),
                "authorizationReceiptAction": "graph-read",
                "agentDelegationHash": qglake_fixture_hash("delegation"),
                "agentSummarySignatureHash": qglake_fixture_hash("summary"),
                "typedidEnvelopeHash": null,
                "typedidProofHash": null,
                "viewVersionReceiptHashes": [qglake_fixture_hash("view-receipt")],
                "replayEventHashes": [qglake_fixture_hash("bootstrap-replay")],
                "openLineageHashes": [qglake_fixture_hash("bootstrap-openlineage")]
            },
            "governedScanProof": {
                "planTaskCount": 1,
                "planGraphEvents": 1,
                "fileTaskCount": 1,
                "deleteFileCount": 1,
                "childPlanTaskCount": 2,
                "plannedReadRestriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [qglake_fixture_hash("scan-policy")]
                },
                "fetchedReadRestriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [qglake_fixture_hash("scan-policy")]
                },
                "plannedRequestedProjection": ["event_id", "occurred_at", "severity", "raw_payload"],
                "plannedEffectiveProjection": ["event_id", "occurred_at", "severity"],
                "plannedRequestedStatsFields": ["event_id", "occurred_at", "severity", "raw_payload"],
                "plannedEffectiveStatsFields": ["event_id", "occurred_at", "severity"],
                "fetchedRequestedStatsFields": ["event_id", "occurred_at", "severity"],
                "fetchedEffectiveStatsFields": ["event_id", "occurred_at", "severity"],
                "fetchedRequiredProjection": ["event_id", "occurred_at", "severity"],
                "fetchedEffectiveProjection": ["event_id", "occurred_at", "severity"],
                "fetchedRequiredFilters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }],
                "plannedReplayEventHashes": [qglake_fixture_hash("scan-plan-replay")],
                "fetchedReplayEventHashes": [qglake_fixture_hash("scan-fetch-replay")],
                "plannedOpenLineageHashes": [qglake_fixture_hash("scan-plan-openlineage")],
                "fetchedOpenLineageHashes": [qglake_fixture_hash("scan-fetch-openlineage")]
            },
            "tableCommitHistoryProof": {
                "commitCount": 1,
                "sequenceNumbers": [1],
                "commitHashes": [qglake_fixture_hash("commit")],
                "principalSubject": "did:example:agent",
                "principalKind": "agent",
                "authorizationReceiptHash": qglake_fixture_hash("table-commits-authorization"),
                "authorizationReceiptAction": "table-load",
                "graphEvents": 1,
                "replayEventHashes": [qglake_fixture_hash("commit-replay")],
                "openLineageHashes": [qglake_fixture_hash("commit-openlineage")]
            },
            "managementProof": {
                "serverCount": 1,
                "serverIds": ["qglake-server"],
                "serverGraphEvents": 1,
                "projectCount": 1,
                "projectIds": ["analytics"],
                "projectGraphEvents": 1,
                "warehouseCount": 1,
                "warehouseNames": ["local"],
                "warehouseProjectId": "analytics",
                "warehouseGraphEvents": 1,
                "policyBindingCount": 1,
                "policyIds": ["agent-columns"],
                "policyGraphEvents": 1,
                "storageProfileCount": 1,
                "storageProfileIds": ["events-local"],
                "storageProfileGraphEvents": 1
            },
            "viewReceiptChainProof": {
                "viewCount": 1,
                "views": [{
                    "stableId": "lakecat:view:local:default:active_customers_view",
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers_view",
                    "viewVersion": 1,
                    "acceptedViewVersion": 1,
                    "acceptedReceiptHash": qglake_fixture_hash("view-receipt"),
                    "acceptedReceiptChainHash": qglake_fixture_hash("view-receipt-chain"),
                    "eventType": "view.upserted",
                    "expectedViewVersion": null,
                    "graphEvents": 1,
                    "replayEventHashes": [qglake_fixture_hash("view-replay")],
                    "openLineageHashes": [qglake_fixture_hash("view-openlineage")]
                }],
                "tombstoneReceipts": [{
                    "stableId": "lakecat:view:local:default:active_customers_view",
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers_view",
                    "expectedViewVersion": 1,
                    "receiptHashes": [qglake_fixture_hash("tombstone")],
                    "replayEventHashes": [qglake_fixture_hash("tombstone-replay")],
                    "openLineageHashes": [qglake_fixture_hash("tombstone-openlineage")]
                }],
                "receiptChains": [{
                    "warehouse": "local",
                    "namespace": ["default"],
                    "verifiedChainCount": 1,
                    "receiptHashes": [qglake_fixture_hash("chain-receipt"), qglake_fixture_hash("tombstone")],
                    "chainHashes": [qglake_fixture_hash("view-receipt-chain")],
                    "replayEventHashes": [qglake_fixture_hash("chain-replay")],
                    "openLineageHashes": [qglake_fixture_hash("chain-openlineage")]
                }]
            },
            "storageProfileUpsertProof": {
                "profileId": "events-local",
                "provider": "file",
                "issuanceMode": "local-file-no-secret",
                "locationPrefixHash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                "secretRefPresent": false,
                "secretRefProvider": null,
                "secretRefHash": null,
                "graphEvents": 1,
                "replayEventHashes": [qglake_fixture_hash("storage-replay")],
                "openLineageHashes": [qglake_fixture_hash("storage-openlineage")]
            },
            "credentialVendingProof": {
                "restricted": {
                    "principalSubject": "did:example:agent",
                    "principalKind": "agent",
                    "credentialCount": 0,
                    "credentialPrefixHashes": [],
                    "rawCredentialExceptionAllowed": false,
                    "blockReason": QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON,
                    "maxCredentialTtlSeconds": 300,
                    "storageProfile": {
                        "profileId": "events-local",
                        "provider": "file",
                        "issuanceMode": "local-file-no-secret",
                        "locationPrefixHash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                        "secretRefPresent": false,
                        "secretRefProvider": null,
                        "secretRefHash": null,
                        "graphEvents": 2
                    },
                    "replayEventHashes": [qglake_fixture_hash("restricted-replay")],
                    "openLineageHashes": [qglake_fixture_hash("restricted-openlineage")]
                },
                "trustedHuman": {
                    "principalSubject": "human:qglake-operator",
                    "principalKind": "human",
                    "credentialCount": 1,
                    "credentialPrefixHashes": [qglake_fixture_hash("human-credential-prefix")],
                    "rawCredentialExceptionAllowed": true,
                    "rawCredentialExceptionReason": QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON,
                    "blockReason": null,
                    "maxCredentialTtlSeconds": 300,
                    "storageProfile": {
                        "profileId": "events-local",
                        "provider": "file",
                        "issuanceMode": "local-file-no-secret",
                        "locationPrefixHash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                        "secretRefPresent": false,
                        "secretRefProvider": null,
                        "secretRefHash": null,
                        "graphEvents": 2
                    },
                    "replayEventHashes": [qglake_fixture_hash("human-replay")],
                    "openLineageHashes": [qglake_fixture_hash("human-openlineage")]
                }
            },
            "replayEvidence": {}
        }
    });
    summary["graphProjectionProof"] = json!({
        "backend": "grust-turso",
        "feature": "grust-turso-local",
        "pathHash": qglake_fixture_hash("grust-turso-path"),
        "tablePrefix": QGLAKE_GRUST_TURSO_TABLE_PREFIX,
        "catalogGraphSink": "GrustCatalogGraphSink<TursoGraphStore>"
    });
    summary["lakecatReplayVerification"]["catalogConfigProof"] = qglake_catalog_config_proof_json();
    qglake_add_management_receipt_hashes(
        &mut summary["lakecatReplayVerification"]["managementProof"],
    );
    qglake_add_governed_scan_identity_evidence(
        &mut summary["lakecatReplayVerification"]["governedScanProof"],
    );
    qglake_add_credential_receipt_evidence(
        &mut summary["lakecatReplayVerification"]["credentialVendingProof"],
    );
    qglake_add_storage_profile_upsert_receipt_evidence(
        &mut summary["lakecatReplayVerification"]["storageProfileUpsertProof"],
    );
    qglake_add_view_receipt_chain_structures(
        &mut summary["lakecatReplayVerification"]["viewReceiptChainProof"],
    );
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewVersionReceiptHashes"] =
        json!([summary["lakecatReplayVerification"]
        ["viewReceiptChainProof"]["views"][0]["acceptedReceiptHash"]
        .clone()]);
    summary
}

pub(crate) fn qglake_catalog_config_proof_json() -> Value {
    let config = CatalogConfigResponse::default();
    json!({
        "defaults": config.defaults,
        "overrides": config.overrides,
        "endpoints": config.endpoints,
        "principalSubject": "did:example:agent",
        "principalKind": "agent",
        "authorizationReceiptHash": qglake_fixture_hash("catalog-config-authorization"),
        "authorizationReceiptAction": "catalog-config",
        "graphEvents": 2,
        "replayEventHashes": [qglake_fixture_hash("catalog-config-replay")],
        "openLineageHashes": [qglake_fixture_hash("catalog-config-openlineage")]
    })
}

pub(crate) fn qglake_add_credential_receipt_evidence(credentials: &mut Value) {
    credentials["restricted"]["authorizationReceiptHash"] =
        json!(qglake_fixture_hash("restricted-credential-authorization"));
    credentials["restricted"]["authorizationReceiptAction"] = json!("credentials-vend");
    credentials["trustedHuman"]["authorizationReceiptHash"] =
        json!(qglake_fixture_hash("human-credential-authorization"));
    credentials["trustedHuman"]["authorizationReceiptAction"] = json!("credentials-vend");
}

pub(crate) fn qglake_add_storage_profile_upsert_receipt_evidence(storage_profile: &mut Value) {
    storage_profile["principalSubject"] = json!("did:example:agent");
    storage_profile["principalKind"] = json!("agent");
    storage_profile["authorizationReceiptHash"] =
        json!(qglake_fixture_hash("storage-profile-upsert-authorization"));
    storage_profile["authorizationReceiptAction"] = json!("storage-profile-manage");
}

pub(crate) fn qglake_add_governed_scan_identity_evidence(governed_scan: &mut Value) {
    governed_scan["plannedPrincipalSubject"] = json!("did:example:agent");
    governed_scan["plannedPrincipalKind"] = json!("agent");
    governed_scan["plannedAuthorizationReceiptHash"] =
        json!(qglake_fixture_hash("scan-planned-authorization"));
    governed_scan["plannedAuthorizationReceiptAction"] = json!("table-plan-scan");
    governed_scan["fetchedPrincipalSubject"] = json!("did:example:agent");
    governed_scan["fetchedPrincipalKind"] = json!("agent");
    governed_scan["fetchedAuthorizationReceiptHash"] =
        json!(qglake_fixture_hash("scan-fetch-authorization"));
    governed_scan["fetchedAuthorizationReceiptAction"] = json!("table-plan-scan");
}

pub(crate) fn qglake_add_view_receipt_chain_structures(view_receipts: &mut Value) {
    let (view_receipt, view_receipt_hash) =
        qglake_fixture_view_receipt("active_customers_view", 1, None, None, "upsert");
    let (tombstone_receipt, tombstone_receipt_hash) = qglake_fixture_view_receipt(
        "active_customers_view",
        1,
        Some(1),
        Some(view_receipt_hash.as_str()),
        "drop",
    );
    let accepted_chain_hash = qglake_fixture_view_chain_hash(
        "active_customers_view",
        1,
        "upsert",
        false,
        &[view_receipt_hash.clone()],
    );
    let tombstone_chain_hash = qglake_fixture_view_chain_hash(
        "active_customers_view",
        1,
        "drop",
        true,
        &[view_receipt_hash.clone(), tombstone_receipt_hash.clone()],
    );
    view_receipts["views"][0]["acceptedReceiptHash"] = json!(view_receipt_hash.clone());
    view_receipts["views"][0]["acceptedReceiptChainHash"] = json!(accepted_chain_hash.clone());
    view_receipts["tombstoneReceipts"][0]["receiptHashes"] =
        json!([tombstone_receipt_hash.clone()]);
    view_receipts["receiptChains"][0]["verifiedChainCount"] = json!(2);
    view_receipts["receiptChains"][0]["receiptHashes"] =
        json!([view_receipt_hash.clone(), tombstone_receipt_hash.clone()]);
    view_receipts["receiptChains"][0]["chainHashes"] =
        json!([accepted_chain_hash.clone(), tombstone_chain_hash.clone()]);
    view_receipts["receiptChains"][0]["chains"] = json!([{
        "stableId": "lakecat:view:local:default:active_customers_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "active_customers_view",
        "chainHash": accepted_chain_hash,
        "chainVerified": true,
        "latestViewVersion": 1,
        "latestOperation": "upsert",
        "tombstoned": false,
        "receiptCount": 1,
        "receipts": [view_receipt.clone()]
    }, {
        "stableId": "lakecat:view:local:default:active_customers_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "active_customers_view",
        "chainHash": tombstone_chain_hash,
        "chainVerified": true,
        "latestViewVersion": 1,
        "latestOperation": "drop",
        "tombstoned": true,
        "receiptCount": 2,
        "receipts": [view_receipt, tombstone_receipt]
    }]);
}

pub(crate) fn qglake_fixture_view_receipt(
    name: &str,
    view_version: u64,
    previous_view_version: Option<u64>,
    previous_receipt_hash: Option<&str>,
    operation: &str,
) -> (Value, String) {
    let view_hash_label = format!("{name}-{view_version}-{operation}-view-hash");
    let view_hash = qglake_fixture_hash(&view_hash_label);
    let recorded_at = if operation == "drop" {
        "2026-06-20T00:00:02Z"
    } else {
        "2026-06-20T00:00:01Z"
    };
    let mut receipt = serde_json::Map::new();
    receipt.insert(
        "stable-id".to_string(),
        json!(format!("lakecat:view:local:default:{name}")),
    );
    receipt.insert("warehouse".to_string(), json!("local"));
    receipt.insert("namespace".to_string(), json!(["default"]));
    receipt.insert("name".to_string(), json!(name));
    receipt.insert("view-version".to_string(), json!(view_version));
    if let Some(previous_view_version) = previous_view_version {
        receipt.insert(
            "previous-view-version".to_string(),
            json!(previous_view_version),
        );
    }
    if let Some(previous_receipt_hash) = previous_receipt_hash {
        receipt.insert(
            "previous-receipt-hash".to_string(),
            json!(previous_receipt_hash),
        );
    }
    receipt.insert("operation".to_string(), json!(operation));
    receipt.insert("view-hash".to_string(), json!(view_hash));
    receipt.insert(
        "principal".to_string(),
        json!({
            "subject": "did:example:agent",
            "kind": "agent",
        }),
    );
    receipt.insert("recorded-at".to_string(), json!(recorded_at));
    let receipt_hash =
        content_hash_json(&Value::Object(receipt)).expect("fixture view receipt hash");
    (
        json!({
            "stableId": format!("lakecat:view:local:default:{name}"),
            "warehouse": "local",
            "namespace": ["default"],
            "name": name,
            "viewVersion": view_version,
            "previousViewVersion": previous_view_version,
            "previousReceiptHash": previous_receipt_hash,
            "operation": operation,
            "viewHash": view_hash,
            "receiptHash": receipt_hash,
            "principalSubject": "did:example:agent",
            "principalKind": "agent",
            "recordedAt": recorded_at,
        }),
        receipt_hash,
    )
}

pub(crate) fn qglake_fixture_view_chain_hash(
    name: &str,
    latest_view_version: u64,
    latest_operation: &str,
    tombstoned: bool,
    receipt_hashes: &[String],
) -> String {
    content_hash_json(&json!({
        "stable-id": format!("lakecat:view:local:default:{name}"),
        "warehouse": "local",
        "namespace": ["default"],
        "name": name,
        "latest-view-version": latest_view_version,
        "latest-operation": latest_operation,
        "tombstoned": tombstoned,
        "receipt-hashes": receipt_hashes,
    }))
    .expect("fixture view chain hash")
}

pub(crate) fn qglake_handoff_summary_json_with_artifacts(dir: &Path) -> Value {
    let bundle = dir.join("lakecat-bootstrap.json");
    let drain = dir.join("lineage-drain.json");
    let import_plan = dir.join("querygraph-import-plan.json");
    let lakecat_replay = dir.join("lakecat-replay.txt");
    let querygraph_verify = dir.join("querygraph-verify.json");
    let querygraph_import = dir.join("querygraph-import.json");
    let lakecat_handoff_verify = dir.join("lakecat-handoff-verify.json");
    let service_log = dir.join("lakecat-service.log");
    let table_commit_history_replay = format!(
        "table commit history commits=1 sequences=1 hashes={} graph_events=1",
        qglake_fixture_hash("commit")
    );
    let mut lakecat_replay_json = json!({
        "schema-version": "lakecat.qglake.replay-verification.v1",
        "status": "verified",
        "table-count": 1,
        "view-count": 1,
        "bundle-hash": qglake_fixture_hash("bundle"),
        "graph-hash": qglake_fixture_hash("graph"),
        "open-lineage-hash": qglake_fixture_hash("openlineage"),
        "querygraph-import-hash": qglake_fixture_hash("querygraph-import"),
        "scan-replay": "scan replay plan_tasks=1 plan_graph_events=1 planned_ttl=300 planned_purpose=qglake-agent-demo file_tasks=1 delete_files=1 child_plan_tasks=2 fetched_ttl=300 fetched_purpose=qglake-agent-demo",
        "credential-replay": "credential replay restricted=blocked:sail-planned-read-required restricted_count=0 restricted_ttl=300 restricted_profile=events-local:file:local-file-no-secret:location_prefix_hash=sha256:2222222222222222222222222222222222222222222222222222222222222222:secret_ref=none:graph_events=2 human=allowed:trusted-human-audited-raw human_count=1 human_ttl=300 human_profile=events-local:file:local-file-no-secret:location_prefix_hash=sha256:2222222222222222222222222222222222222222222222222222222222222222:secret_ref=none:graph_events=2",
        "standards": [
            "Iceberg REST",
            "Croissant",
            "CDIF",
            "OSI handoff",
            "ODRL",
            "Grust catalog graph",
            "OpenLineage"
        ],
        "replay-evidence": {
            "requestIdentity": {
                "principalSubject": "did:example:agent",
                "principalKind": "agent",
                "requestIdentitySource": "x-lakecat-agent-did",
                "requestIdentityState": "unverified",
                "authorizationReceiptHash": qglake_fixture_hash("identity"),
                "typedidEnvelopeHash": null,
                "typedidProofHash": null
            },
            "queryGraphBootstrap": {
                "bundleHash": qglake_fixture_hash("bundle"),
                "graphHash": qglake_fixture_hash("graph"),
                "openLineageHash": qglake_fixture_hash("openlineage"),
                "queryGraphImportHash": qglake_fixture_hash("querygraph-import"),
                "tableArtifactCount": 1,
                "viewArtifactCount": 1,
                "policyBindingCount": 1,
                "standards": [
                    "Iceberg REST",
                    "Croissant",
                    "CDIF",
                    "OSI handoff",
                    "ODRL",
                    "Grust catalog graph",
                    "OpenLineage"
                ],
                "principalSubject": "did:example:agent",
                "principalKind": "agent",
                "requestIdentitySource": "x-lakecat-agent-did",
                "requestIdentityState": "unverified",
                "authorizationReceiptHash": qglake_fixture_hash("identity"),
                "agentDelegationHash": qglake_fixture_hash("delegation"),
                "agentSummarySignatureHash": qglake_fixture_hash("summary"),
                "typedidEnvelopeHash": null,
                "typedidProofHash": null,
                "viewVersionReceiptHashes": [qglake_fixture_hash("view-receipt")],
                "replayEventHashes": [qglake_fixture_hash("bootstrap-replay")],
                "openLineageHashes": [qglake_fixture_hash("bootstrap-openlineage")]
            },
            "scan": {
                "planTaskCount": 1,
                "planGraphEvents": 1,
                "fileTaskCount": 1,
                "deleteFileCount": 1,
                "childPlanTaskCount": 2,
                "plannedReadRestriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [qglake_fixture_hash("scan-policy")]
                },
                "fetchedReadRestriction": {
                    "allowed-columns": ["event_id", "occurred_at", "severity"],
                    "row-predicate": {
                        "type": "not-eq",
                        "term": "severity",
                        "value": "debug"
                    },
                    "purpose": "qglake-agent-demo",
                    "max-credential-ttl-seconds": 300,
                    "policy-hashes": [qglake_fixture_hash("scan-policy")]
                },
                "plannedRequestedProjection": ["event_id", "occurred_at", "severity", "raw_payload"],
                "plannedEffectiveProjection": ["event_id", "occurred_at", "severity"],
                "plannedRequestedStatsFields": ["event_id", "occurred_at", "severity", "raw_payload"],
                "plannedEffectiveStatsFields": ["event_id", "occurred_at", "severity"],
                "fetchedRequestedStatsFields": ["event_id", "occurred_at", "severity"],
                "fetchedEffectiveStatsFields": ["event_id", "occurred_at", "severity"],
                "fetchedRequiredProjection": ["event_id", "occurred_at", "severity"],
                "fetchedEffectiveProjection": ["event_id", "occurred_at", "severity"],
                "fetchedRequiredFilters": [{
                    "type": "not-eq",
                    "term": "severity",
                    "value": "debug"
                }],
                "plannedReplayEventHashes": [qglake_fixture_hash("scan-plan-replay")],
                "fetchedReplayEventHashes": [qglake_fixture_hash("scan-fetch-replay")],
                "plannedOpenLineageHashes": [qglake_fixture_hash("scan-plan-openlineage")],
                "fetchedOpenLineageHashes": [qglake_fixture_hash("scan-fetch-openlineage")]
            },
            "tableCommitHistory": {
                "commitCount": 1,
                "sequenceNumbers": [1],
                "commitHashes": [qglake_fixture_hash("commit")],
                "principalSubject": "did:example:agent",
                "principalKind": "agent",
                "authorizationReceiptHash": qglake_fixture_hash("table-commits-authorization"),
                "authorizationReceiptAction": "table-load",
                "graphEvents": 1,
                "replayEventHashes": [qglake_fixture_hash("commit-replay")],
                "openLineageHashes": [qglake_fixture_hash("commit-openlineage")]
            },
            "management": {
                "serverCount": 1,
                "serverGraphEvents": 1,
                "projectCount": 1,
                "projectGraphEvents": 1,
                "warehouseCount": 1,
                "warehouseProjectId": "analytics",
                "warehouseGraphEvents": 1,
                "policyBindingCount": 1,
                "policyGraphEvents": 1,
                "storageProfileCount": 1,
                "storageProfileGraphEvents": 1,
                "storageProfileUpsert": {
                    "profileId": "events-local",
                    "provider": "file",
                    "issuanceMode": "local-file-no-secret",
                    "locationPrefixHash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                    "secretRefPresent": false,
                    "secretRefProvider": null,
                    "secretRefHash": null,
                    "graphEvents": 1,
                    "replayEventHashes": [qglake_fixture_hash("storage-replay")],
                    "openLineageHashes": [qglake_fixture_hash("storage-openlineage")]
                }
            },
            "views": {
                "viewCount": 1,
                "views": [{
                    "stableId": "lakecat:view:local:default:active_customers_view",
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers_view",
                    "viewVersion": 1,
                    "acceptedViewVersion": 1,
                    "acceptedReceiptHash": qglake_fixture_hash("view-receipt"),
                    "acceptedReceiptChainHash": qglake_fixture_hash("view-receipt-chain"),
                    "eventType": "view.upserted",
                    "expectedViewVersion": null,
                    "graphEvents": 1,
                    "replayEventHashes": [qglake_fixture_hash("view-replay")],
                    "openLineageHashes": [qglake_fixture_hash("view-openlineage")]
                }],
                "tombstoneReceipts": [{
                    "stableId": "lakecat:view:local:default:active_customers_view",
                    "warehouse": "local",
                    "namespace": ["default"],
                    "name": "active_customers_view",
                    "expectedViewVersion": 1,
                    "receiptHashes": [qglake_fixture_hash("tombstone")],
                    "replayEventHashes": [qglake_fixture_hash("tombstone-replay")],
                    "openLineageHashes": [qglake_fixture_hash("tombstone-openlineage")]
                }],
                "receiptChains": [{
                    "warehouse": "local",
                    "namespace": ["default"],
                    "verifiedChainCount": 1,
                    "receiptHashes": [qglake_fixture_hash("chain-receipt"), qglake_fixture_hash("tombstone")],
                    "chainHashes": [qglake_fixture_hash("view-receipt-chain")],
                    "replayEventHashes": [qglake_fixture_hash("chain-replay")],
                    "openLineageHashes": [qglake_fixture_hash("chain-openlineage")]
                }]
            },
            "credentials": {
                "restricted": {
                    "principalSubject": "did:example:agent",
                    "principalKind": "agent",
                    "credentialCount": 0,
                    "credentialPrefixHashes": [],
                    "rawCredentialExceptionAllowed": false,
                    "blockReason": QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON,
                    "maxCredentialTtlSeconds": 300,
                    "storageProfile": {
                        "profileId": "events-local",
                        "provider": "file",
                        "issuanceMode": "local-file-no-secret",
                        "locationPrefixHash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                        "secretRefPresent": false,
                        "secretRefProvider": null,
                        "secretRefHash": null,
                        "graphEvents": 2
                    },
                    "replayEventHashes": [qglake_fixture_hash("restricted-replay")],
                    "openLineageHashes": [qglake_fixture_hash("restricted-openlineage")]
                },
                "trustedHuman": {
                    "principalSubject": "human:qglake-operator",
                    "principalKind": "human",
                    "credentialCount": 1,
                    "credentialPrefixHashes": [qglake_fixture_hash("human-credential-prefix")],
                    "rawCredentialExceptionAllowed": true,
                    "rawCredentialExceptionReason": QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON,
                    "blockReason": null,
                    "maxCredentialTtlSeconds": 300,
                    "storageProfile": {
                        "profileId": "events-local",
                        "provider": "file",
                        "issuanceMode": "local-file-no-secret",
                        "locationPrefixHash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                        "secretRefPresent": false,
                        "secretRefProvider": null,
                        "secretRefHash": null,
                        "graphEvents": 2
                    },
                    "replayEventHashes": [qglake_fixture_hash("human-replay")],
                    "openLineageHashes": [qglake_fixture_hash("human-openlineage")]
                }
            }
        }
    });
    lakecat_replay_json["replay-evidence"]["catalogConfig"] = qglake_catalog_config_proof_json();
    lakecat_replay_json["replay-evidence"]["requestIdentity"]["authorizationReceiptAction"] =
        json!("lineage-read");
    lakecat_replay_json["replay-evidence"]["queryGraphBootstrap"]["authorizationReceiptAction"] =
        json!("graph-read");
    qglake_add_governed_scan_identity_evidence(&mut lakecat_replay_json["replay-evidence"]["scan"]);
    qglake_add_credential_receipt_evidence(
        &mut lakecat_replay_json["replay-evidence"]["credentials"],
    );
    qglake_add_storage_profile_upsert_receipt_evidence(
        &mut lakecat_replay_json["replay-evidence"]["management"]["storageProfileUpsert"],
    );
    lakecat_replay_json["management-replay"] = json!(format!(
        "management replay servers=1 projects=1 warehouses=1 policies=1 policy_upserts=1 policy=agent-columns:odrl_hash={} storage_profiles=1 storage_profile_upserts=1 credential_root=events-local:file:local-file-no-secret:location_prefix_hash=sha256:2222222222222222222222222222222222222222222222222222222222222222:secret_ref=none",
        qglake_fixture_hash("policy-odrl")
    ));
    lakecat_replay_json["table-commit-history-replay"] = json!(table_commit_history_replay);
    lakecat_replay_json["replay-evidence"]["management"]["serverIds"] = json!(["qglake-server"]);
    lakecat_replay_json["replay-evidence"]["management"]["projectIds"] = json!(["analytics"]);
    lakecat_replay_json["replay-evidence"]["management"]["warehouseNames"] = json!(["local"]);
    lakecat_replay_json["replay-evidence"]["management"]["policyIds"] = json!(["agent-columns"]);
    lakecat_replay_json["replay-evidence"]["management"]["storageProfileIds"] =
        json!(["events-local"]);
    qglake_add_management_receipt_hashes(&mut lakecat_replay_json["replay-evidence"]["management"]);
    qglake_add_view_receipt_chain_structures(&mut lakecat_replay_json["replay-evidence"]["views"]);
    lakecat_replay_json["replay-evidence"]["queryGraphBootstrap"]["viewVersionReceiptHashes"] =
        json!([
            lakecat_replay_json["replay-evidence"]["views"]["views"][0]["acceptedReceiptHash"]
                .clone()
        ]);
    let querygraph_capture_json = json!({
        "warehouse": "local",
        "table-count": 1,
        "view-count": 1,
        "verified-tables": [
            "lakecat:table:local:default:events"
        ],
        "verified-views": [
            "lakecat:view:local:default:active_customers_view"
        ],
        "bundle-hash": qglake_fixture_hash("bundle"),
        "graph-hash": qglake_fixture_hash("graph"),
        "open-lineage-hash": qglake_fixture_hash("openlineage"),
        "querygraph-import-hash": qglake_fixture_hash("querygraph-import"),
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
    let lakecat_replay_bytes =
        serde_json::to_vec_pretty(&lakecat_replay_json).expect("LakeCat replay JSON bytes");
    let querygraph_verify_bytes =
        serde_json::to_vec_pretty(&querygraph_capture_json).expect("verify JSON bytes");
    let querygraph_import_bytes =
        serde_json::to_vec_pretty(&querygraph_capture_json).expect("import JSON bytes");
    let lineage_drain_bytes =
        serde_json::to_vec_pretty(&qglake_handoff_lineage_drain_with_config())
            .expect("lineage drain JSON bytes");
    fs::write(&bundle, b"bundle").expect("write bundle");
    fs::write(&drain, &lineage_drain_bytes).expect("write drain");
    fs::write(&import_plan, b"import-plan").expect("write import plan");
    fs::write(&lakecat_replay, &lakecat_replay_bytes).expect("write LakeCat replay");
    fs::write(&querygraph_verify, &querygraph_verify_bytes).expect("write QueryGraph verify");
    fs::write(&querygraph_import, &querygraph_import_bytes).expect("write QueryGraph import");
    fs::write(&service_log, b"service log").expect("write service log");

    let mut summary = qglake_handoff_summary_json();
    summary["artifacts"] = json!({
        "bundle": {
            "path": bundle,
            "sha256": content_hash_bytes(b"bundle")
        },
        "lineageDrain": {
            "path": drain,
            "sha256": content_hash_bytes(&lineage_drain_bytes)
        },
        "querygraphImportPlan": {
            "path": import_plan,
            "sha256": content_hash_bytes(b"import-plan")
        },
        "lakecatReplayOutput": lakecat_replay,
        "lakecatHandoffVerifyOutput": lakecat_handoff_verify,
        "querygraphVerifyOutput": querygraph_verify,
        "querygraphImportOutput": querygraph_import,
        "capturedOutputs": {
            "lakecatReplay": {
                "path": lakecat_replay,
                "sha256": content_hash_bytes(&lakecat_replay_bytes)
            },
            "querygraphVerify": {
                "path": querygraph_verify,
                "sha256": content_hash_bytes(&querygraph_verify_bytes)
            },
            "querygraphImport": {
                "path": querygraph_import,
                "sha256": content_hash_bytes(&querygraph_import_bytes)
            }
        },
        "serviceLog": service_log,
        "serviceLogHash": content_hash_bytes(b"service log")
    });
    summary
}

pub(crate) fn qglake_bind_handoff_verify_output_artifact(dir: &Path, summary: &mut Value) -> Value {
    let lineage_drain_semantics =
        qglake_handoff_lineage_drain_summary_fields(&dir.join("lineage-drain.json"))
            .expect("lineage drain summary fields");
    let (graph_nodes, graph_edges) = qglake_handoff_fixture_graph_counts(summary);
    let output = json!({
        "schemaVersion": "lakecat.qglake.handoff-verification.v1",
        "status": "verified",
        "principal": summary["principal"].clone(),
        "catalogUrl": summary["catalogUrl"].clone(),
        "warehouse": summary["warehouse"].clone(),
        "namespace": summary["namespace"].clone(),
        "table": summary["table"].clone(),
        "tableCount": summary["querygraphVerification"]["tableCount"].clone(),
        "viewCount": summary["querygraphVerification"]["viewCount"].clone(),
        "verifiedTables": summary["querygraphVerification"]["verifiedTables"].clone(),
        "verifiedViews": summary["querygraphVerification"]["verifiedViews"].clone(),
        "standards": summary["querygraphVerification"]["standards"].clone(),
        "requestIdentityProof": summary["lakecatReplayVerification"]["requestIdentityProof"].clone(),
        "queryGraphBootstrapProof": summary["lakecatReplayVerification"]["queryGraphBootstrapProof"].clone(),
        "artifactFiles": {
            "bundle": {
                "sha256": summary["artifacts"]["bundle"]["sha256"].clone()
            },
            "lineageDrain": {
                "sha256": summary["artifacts"]["lineageDrain"]["sha256"].clone()
            },
            "querygraphImportPlan": {
                "sha256": summary["artifacts"]["querygraphImportPlan"]["sha256"].clone()
            },
            "capturedOutputs": {
                "lakecatReplay": {
                    "sha256": summary["artifacts"]["capturedOutputs"]["lakecatReplay"]["sha256"].clone()
                },
                "querygraphVerify": {
                    "sha256": summary["artifacts"]["capturedOutputs"]["querygraphVerify"]["sha256"].clone()
                },
                "querygraphImport": {
                    "sha256": summary["artifacts"]["capturedOutputs"]["querygraphImport"]["sha256"].clone()
                }
            },
            "serviceLogHash": summary["artifacts"]["serviceLogHash"].clone()
        },
        "capturedOutputSemantics": {
            "lakecatReplay": {
                "requestIdentityProof": summary["lakecatReplayVerification"]["requestIdentityProof"].clone(),
                "queryGraphBootstrapProof": summary["lakecatReplayVerification"]["queryGraphBootstrapProof"].clone(),
                "governedScanProof": summary["lakecatReplayVerification"]["governedScanProof"].clone(),
                "catalogConfigProof": summary["lakecatReplayVerification"]["catalogConfigProof"].clone(),
                "tableCommitHistoryProof": summary["lakecatReplayVerification"]["tableCommitHistoryProof"].clone(),
                "viewReceiptChainProof": summary["lakecatReplayVerification"]["viewReceiptChainProof"].clone(),
                "managementProof": summary["lakecatReplayVerification"]["managementProof"].clone(),
                "storageProfileUpsertProof": summary["lakecatReplayVerification"]["storageProfileUpsertProof"].clone(),
                "credentialVendingProof": summary["lakecatReplayVerification"]["credentialVendingProof"].clone()
            },
            "querygraphVerify": {
                "tableCount": summary["querygraphVerification"]["tableCount"].clone(),
                "viewCount": summary["querygraphVerification"]["viewCount"].clone(),
                "verifiedTables": summary["querygraphVerification"]["verifiedTables"].clone(),
                "verifiedViews": summary["querygraphVerification"]["verifiedViews"].clone(),
                "bundleHash": summary["querygraphVerification"]["bundleHash"].clone(),
                "graphHash": summary["querygraphVerification"]["graphHash"].clone(),
                "openLineageHash": summary["querygraphVerification"]["openLineageHash"].clone(),
                "queryGraphImportHash": summary["querygraphVerification"]["querygraphImportHash"].clone(),
                "standards": summary["querygraphVerification"]["standards"].clone()
            },
            "querygraphImport": {
                "tableCount": summary["querygraphImportVerification"]["tableCount"].clone(),
                "viewCount": summary["querygraphImportVerification"]["viewCount"].clone(),
                "verifiedTables": summary["querygraphImportVerification"]["verifiedTables"].clone(),
                "verifiedViews": summary["querygraphImportVerification"]["verifiedViews"].clone(),
                "bundleHash": summary["querygraphImportVerification"]["bundleHash"].clone(),
                "graphHash": summary["querygraphImportVerification"]["graphHash"].clone(),
                "openLineageHash": summary["querygraphImportVerification"]["openLineageHash"].clone(),
                "queryGraphImportHash": summary["querygraphImportVerification"]["querygraphImportHash"].clone(),
                "standards": summary["querygraphImportVerification"]["standards"].clone()
            }
        },
        "bundleArtifactSemantics": {
            "tableCount": summary["querygraphVerification"]["tableCount"].clone(),
            "viewCount": summary["querygraphVerification"]["viewCount"].clone(),
            "verifiedTables": summary["querygraphVerification"]["verifiedTables"].clone(),
            "verifiedViews": summary["querygraphVerification"]["verifiedViews"].clone(),
            "bundleHash": summary["querygraphVerification"]["bundleHash"].clone(),
            "graphHash": summary["querygraphVerification"]["graphHash"].clone(),
            "openLineageHash": summary["querygraphVerification"]["openLineageHash"].clone(),
            "queryGraphImportHash": summary["querygraphVerification"]["querygraphImportHash"].clone(),
            "standards": summary["querygraphVerification"]["standards"].clone(),
            "graphNodes": graph_nodes,
            "graphEdges": graph_edges
        },
        "querygraphImportPlanSemantics": {
            "tableCount": summary["querygraphImportVerification"]["tableCount"].clone(),
            "viewCount": summary["querygraphImportVerification"]["viewCount"].clone(),
            "verifiedTables": summary["querygraphImportVerification"]["verifiedTables"].clone(),
            "verifiedViews": summary["querygraphImportVerification"]["verifiedViews"].clone(),
            "bundleHash": summary["querygraphImportVerification"]["bundleHash"].clone(),
            "graphHash": summary["querygraphImportVerification"]["graphHash"].clone(),
            "openLineageHash": summary["querygraphImportVerification"]["openLineageHash"].clone(),
            "queryGraphImportHash": summary["querygraphImportVerification"]["querygraphImportHash"].clone(),
            "standards": summary["querygraphImportVerification"]["standards"].clone(),
            "graphNodes": graph_nodes,
            "graphEdges": graph_edges
        },
        "lineageDrainArtifactSemantics": {
            "delivered": 15,
            "eventTypes": [
                "querygraph.bootstrap",
                "credentials.vend-attempted",
                "credentials.vend-attempted",
                "view.upserted",
                "policy-binding.listed",
                "policy-binding.upserted",
                "storage-profile.listed",
                "storage-profile.upserted",
                "server.listed",
                "project.listed",
                "warehouse.listed",
                "table.commits-listed",
                "table.scan-planned",
                "table.scan-tasks-fetched",
                "catalog.config-read"
            ],
            "graphEvents": 17,
            "lineageEvents": 15,
            "catalogConfigProof": lineage_drain_semantics["catalogConfigProof"].clone(),
            "principalSubject": summary["lakecatReplayVerification"]["requestIdentityProof"]["principalSubject"].clone(),
            "principalKind": summary["lakecatReplayVerification"]["requestIdentityProof"]["principalKind"].clone(),
            "authorizationReceiptHash": summary["lakecatReplayVerification"]["requestIdentityProof"]["authorizationReceiptHash"].clone(),
            "authorizationReceiptAction": summary["lakecatReplayVerification"]["requestIdentityProof"]["authorizationReceiptAction"].clone(),
            "requestIdentitySource": summary["lakecatReplayVerification"]["requestIdentityProof"]["requestIdentitySource"].clone(),
            "requestIdentityState": summary["lakecatReplayVerification"]["requestIdentityProof"]["requestIdentityState"].clone(),
            "typedidEnvelopeHash": summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidEnvelopeHash"].clone(),
            "typedidProofHash": summary["lakecatReplayVerification"]["requestIdentityProof"]["typedidProofHash"].clone(),
            "tableCount": summary["querygraphVerification"]["tableCount"].clone(),
            "viewCount": summary["querygraphVerification"]["viewCount"].clone(),
            "verifiedTables": summary["querygraphVerification"]["verifiedTables"].clone(),
            "verifiedViews": summary["querygraphVerification"]["verifiedViews"].clone(),
            "bundleHash": summary["querygraphVerification"]["bundleHash"].clone(),
            "graphHash": summary["querygraphVerification"]["graphHash"].clone(),
            "openLineageHash": summary["querygraphVerification"]["openLineageHash"].clone(),
            "queryGraphImportHash": summary["querygraphVerification"]["querygraphImportHash"].clone(),
            "standards": summary["querygraphVerification"]["standards"].clone()
        },
    });
    let mut output = output;
    output["graphProjectionProof"] = summary["graphProjectionProof"].clone();
    let bytes = serde_json::to_vec_pretty(&output).expect("handoff verify JSON bytes");
    fs::write(dir.join("lakecat-handoff-verify.json"), &bytes)
        .expect("write handoff verify output");
    summary["artifacts"]["lakecatHandoffVerifyOutputHash"] = json!(content_hash_bytes(&bytes));
    output
}

pub(crate) fn qglake_handoff_fixture_graph_counts(summary: &Value) -> (usize, usize) {
    let tables = summary["querygraphImportVerification"]["verifiedTables"]
        .as_array()
        .expect("verified tables array")
        .iter()
        .map(|stable_id| {
            let mut table = qglake_querygraph_projection(qglake_odrl_policy("events"));
            table.stable_id = stable_id
                .as_str()
                .expect("verified table stable id")
                .to_string();
            table
        })
        .collect::<Vec<_>>();
    let views = summary["querygraphImportVerification"]["verifiedViews"]
        .as_array()
        .expect("verified views array")
        .iter()
        .map(|stable_id| lakecat_querygraph::QueryGraphViewProjection {
            stable_id: stable_id
                .as_str()
                .expect("verified view stable id")
                .to_string(),
            warehouse: "local".to_string(),
            namespace: vec!["default".to_string()],
            name: "active_customers_view".to_string(),
            view_version: 1,
            sql: "select * from events".to_string(),
            dialect: "ansi".to_string(),
            schema_version: Some(1),
            columns: json!([]),
            properties: json!({}),
            osi: json!({}),
        })
        .collect::<Vec<_>>();
    let warehouse = lakecat_core::WarehouseName::new("local").unwrap();
    let graph = lakecat_querygraph::QueryGraphCatalogGraph::from_tables_and_views_for_warehouse(
        &warehouse,
        &tables,
        &views,
        &lakecat_querygraph::QueryGraphTenantProjection::default(),
    );
    (graph.nodes.len(), graph.edges.len())
}

pub(crate) fn qglake_handoff_summary_json_with_verified_bundle(
    dir: &Path,
) -> (Value, QueryGraphBootstrap) {
    let mut summary = qglake_handoff_summary_json_with_artifacts(dir);
    let projection = qglake_querygraph_projection(qglake_odrl_policy("events"));
    let output = serde_json::json!({
        "name": "events",
        "facets": {
            "dataSource": {
                "uri": QGLAKE_TEST_LOCATION
            },
            "queryGraph_catalog": {
                "stableId": projection.stable_id.clone(),
                "metadataLocation": projection.metadata_location.clone()
            }
        }
    });
    let bundle = qglake_querygraph_bundle(vec![projection], vec![output]);
    qglake_write_handoff_bundle_artifact(dir, &mut summary, &bundle);
    (summary, bundle)
}

pub(crate) fn qglake_write_handoff_bundle_artifact(
    dir: &Path,
    summary: &mut Value,
    bundle: &QueryGraphBootstrap,
) {
    let verification = bundle.verify_manifest().expect("bundle should verify");
    let bytes = serde_json::to_vec_pretty(bundle).expect("bundle JSON");
    fs::write(dir.join("lakecat-bootstrap.json"), &bytes).expect("write bundle");
    summary["artifacts"]["bundle"]["sha256"] = json!(content_hash_bytes(&bytes));
    for section in ["querygraphVerification", "querygraphImportVerification"] {
        summary[section]["tableCount"] = json!(verification.table_count);
        summary[section]["viewCount"] = json!(verification.view_count);
        summary[section]["verifiedTables"] = json!(verification.verified_tables);
        summary[section]["verifiedViews"] = json!(verification.verified_views);
        summary[section]["bundleHash"] = json!(verification.bundle_hash);
        summary[section]["graphHash"] = json!(verification.graph_hash);
        summary[section]["openLineageHash"] = json!(verification.open_lineage_hash);
        summary[section]["querygraphImportHash"] = json!(verification.querygraph_import_hash);
        summary[section]["standards"] = json!(verification.standards);
    }
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["bundleHash"] =
        json!(verification.bundle_hash);
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["graphHash"] =
        json!(verification.graph_hash);
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["openLineageHash"] =
        json!(verification.open_lineage_hash);
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["queryGraphImportHash"] =
        json!(verification.querygraph_import_hash);
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["tableArtifactCount"] =
        json!(verification.table_count);
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewArtifactCount"] =
        json!(verification.view_count);
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["standards"] =
        json!(verification.standards);
}

pub(crate) fn qglake_write_handoff_import_plan_artifact(dir: &Path, summary: &mut Value) -> Value {
    let import = summary["querygraphImportVerification"].clone();
    let (graph_nodes, graph_edges) = qglake_handoff_fixture_graph_counts(summary);
    let tables = import["verifiedTables"]
        .as_array()
        .expect("verified tables array")
        .iter()
        .map(|stable_id| {
            json!({
                "stable-id": stable_id,
                "croissant-name": "events",
                "cdif-title": "events",
                "osi-model": "events",
                "odrl-policy": "events#odrl"
            })
        })
        .collect::<Vec<_>>();
    let views = import["verifiedViews"]
        .as_array()
        .expect("verified views array")
        .iter()
        .map(|stable_id| {
            json!({
                "stable-id": stable_id,
                "name": "active_customers_view",
                "view-version": 1,
                "dialect": "ansi",
                "osi-model": "active_customers_view"
            })
        })
        .collect::<Vec<_>>();
    let plan = json!({
        "verification": {
            "warehouse": summary["warehouse"],
            "table-count": import["tableCount"],
            "view-count": import["viewCount"],
            "verified-tables": import["verifiedTables"],
            "verified-views": import["verifiedViews"],
            "bundle-hash": import["bundleHash"],
            "graph-hash": import["graphHash"],
            "open-lineage-hash": import["openLineageHash"],
            "querygraph-import-hash": import["querygraphImportHash"],
            "standards": import["standards"]
        },
        "graph-nodes": graph_nodes,
        "graph-edges": graph_edges,
        "tables": tables,
        "views": views
    });
    let bytes = serde_json::to_vec_pretty(&plan).expect("import plan JSON");
    fs::write(dir.join("querygraph-import-plan.json"), &bytes).expect("write import plan");
    summary["artifacts"]["querygraphImportPlan"]["sha256"] = json!(content_hash_bytes(&bytes));
    plan
}

pub(crate) fn qglake_handoff_lineage_verification() -> QueryGraphBootstrapVerification {
    QueryGraphBootstrapVerification {
        warehouse: "local".to_string(),
        table_count: 1,
        view_count: 1,
        verified_tables: vec!["lakecat:table:local:default:events".to_string()],
        verified_views: vec!["lakecat:view:local:default:active_customers_view".to_string()],
        verified_view_versions: BTreeMap::from([(
            "lakecat:view:local:default:active_customers_view".to_string(),
            1,
        )]),
        verified_view_receipt_hashes: BTreeMap::from([(
            "lakecat:view:local:default:active_customers_view".to_string(),
            qglake_fixture_hash("view-receipt"),
        )]),
        verified_view_receipt_chain_hashes: BTreeMap::from([(
            "lakecat:view:local:default:active_customers_view".to_string(),
            qglake_fixture_hash("view-receipt-chain"),
        )]),
        bundle_hash: qglake_fixture_hash("bundle"),
        graph_hash: qglake_fixture_hash("graph"),
        open_lineage_hash: qglake_fixture_hash("openlineage"),
        querygraph_import_hash: qglake_fixture_hash("querygraph-import"),
        standards: qglake_lineage_standards(),
    }
}

pub(crate) fn qglake_handoff_lineage_drain() -> LineageDrainResponse {
    let verification = qglake_handoff_lineage_verification();
    let mut view = qglake_view_lineage_summary();
    view.view_name = Some("active_customers_view".to_string());
    view.view_stable_id = Some("lakecat:view:local:default:active_customers_view".to_string());
    view.view_version = Some(1);
    view.expected_view_version = None;
    view.replay_event_hashes = vec![qglake_fixture_hash("view-replay")];
    view.replay_open_lineage_hashes = vec![qglake_fixture_hash("view-openlineage")];

    LineageDrainResponse {
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
        authorization_receipt_hash: Some(qglake_fixture_hash("identity")),
        authorization_receipt_action: Some("lineage-read".to_string()),
        request_identity_state: Some("unverified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        events: vec![
            qglake_bootstrap_lineage_summary_for(&verification, 1),
            qglake_restricted_credential_summary(),
            qglake_human_credential_summary(),
            view,
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
    }
}

pub(crate) fn qglake_handoff_lineage_drain_with_config() -> LineageDrainResponse {
    let mut drain = qglake_handoff_lineage_drain();
    let config = qglake_catalog_config_lineage_summary();
    drain.delivered += 1;
    drain.graph_events += config.graph_events;
    drain.lineage_events += config.lineage_events;
    drain.event_types.push(config.event_type.clone());
    drain.events.push(config);
    drain
}

pub(crate) fn qglake_lineage_drain_from_summaries(
    events: Vec<LineageDrainEventSummary>,
) -> LineageDrainResponse {
    LineageDrainResponse {
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
    }
}

pub(crate) fn qglake_write_handoff_lineage_drain_artifact(
    dir: &Path,
    summary: &mut Value,
    drain: &LineageDrainResponse,
) {
    let verification = qglake_handoff_lineage_verification();
    for section in ["querygraphVerification", "querygraphImportVerification"] {
        summary[section]["tableCount"] = json!(verification.table_count);
        summary[section]["viewCount"] = json!(verification.view_count);
        summary[section]["verifiedTables"] = json!(verification.verified_tables);
        summary[section]["verifiedViews"] = json!(verification.verified_views);
        summary[section]["bundleHash"] = json!(verification.bundle_hash);
        summary[section]["graphHash"] = json!(verification.graph_hash);
        summary[section]["openLineageHash"] = json!(verification.open_lineage_hash);
        summary[section]["querygraphImportHash"] = json!(verification.querygraph_import_hash);
        summary[section]["standards"] = json!(verification.standards);
    }
    let replay = qglake_replay_verification_json(
        &verification,
        qglake_scan_replay_line(drain),
        qglake_management_replay_line(drain),
        qglake_credential_replay_line(drain, Some("did:example:agent")),
        qglake_table_commit_history_replay_line(drain),
        qglake_replay_evidence_json(drain, Some("did:example:agent"), &verification),
    );
    summary["lakecatReplayVerification"] = json!({
        "schemaVersion": replay["schema-version"],
        "status": replay["status"],
        "matchesQueryGraph": true,
        "requestIdentityProof": replay["replay-evidence"]["requestIdentity"],
        "queryGraphBootstrapProof": replay["replay-evidence"]["queryGraphBootstrap"],
        "catalogConfigProof": replay["replay-evidence"]["catalogConfig"],
        "governedScanProof": replay["replay-evidence"]["scan"],
        "tableCommitHistoryProof": replay["replay-evidence"]["tableCommitHistory"],
        "viewReceiptChainProof": replay["replay-evidence"]["views"],
        "managementProof": replay["replay-evidence"]["management"],
        "storageProfileUpsertProof": replay["replay-evidence"]["management"]["storageProfileUpsert"],
        "credentialVendingProof": replay["replay-evidence"]["credentials"],
        "replayEvidence": replay["replay-evidence"],
    });
    let bytes = serde_json::to_vec_pretty(drain).expect("lineage drain JSON");
    fs::write(dir.join("lineage-drain.json"), &bytes).expect("write lineage drain");
    summary["artifacts"]["lineageDrain"]["sha256"] = json!(content_hash_bytes(&bytes));
}

pub(crate) fn qglake_resync_bundle_hashes(bundle: &mut QueryGraphBootstrap) {
    let graph_hash = content_hash_json(&serde_json::to_value(&bundle.graph).unwrap()).unwrap();
    bundle.manifest.graph_hash = graph_hash.clone();
    bundle.open_lineage["run"]["facets"]["queryGraph_semanticBundle"]["graphHash"] =
        json!(graph_hash);
    bundle.manifest.open_lineage_hash = content_hash_json(&bundle.open_lineage).unwrap();
    if let Some(import) = bundle.manifest.querygraph_import.as_mut() {
        import.graph_hash = bundle.manifest.graph_hash.clone();
    }
    let import_hash = qglake_querygraph_import_hash(
        &bundle.warehouse,
        &bundle.manifest,
        &bundle.tables,
        &bundle.graph,
        &bundle.open_lineage,
    );
    if let Some(import) = bundle.manifest.querygraph_import.as_mut() {
        import.table_only_bundle_hash = import_hash;
    }
    bundle.bundle_hash = content_hash_json(&serde_json::json!({
        "warehouse": bundle.warehouse.as_str(),
        "manifest": &bundle.manifest,
        "tables": &bundle.tables,
        "views": &bundle.views,
        "graph": &bundle.graph,
        "openLineage": &bundle.open_lineage,
    }))
    .unwrap();
}

pub(crate) fn qglake_temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("lakecat-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

pub(crate) fn set_qglake_secret_ref_backed_profile(profile: &mut Value, secret_ref_hash: &str) {
    profile["provider"] = json!("s3");
    profile["issuanceMode"] = json!("short-lived-secret-ref");
    profile["secretRefPresent"] = json!(true);
    profile["secretRefProvider"] = json!("typesec");
    profile["secretRefHash"] = json!(secret_ref_hash);
}

pub(crate) fn qglake_set_two_receipt_view_chain(summary: &mut Value) {
    let (receipt_v1, receipt_hash_v1) =
        qglake_fixture_view_receipt("active_customers_view", 1, None, None, "upsert");
    let (receipt_v2, receipt_hash_v2) = qglake_fixture_view_receipt(
        "active_customers_view",
        2,
        Some(1),
        Some(receipt_hash_v1.as_str()),
        "upsert",
    );
    let chain_hash = qglake_fixture_view_chain_hash(
        "active_customers_view",
        2,
        "upsert",
        false,
        &[receipt_hash_v1.clone(), receipt_hash_v2.clone()],
    );
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["viewVersion"] =
        json!(2);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedViewVersion"] =
        json!(2);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptHash"] =
        json!(receipt_hash_v2.clone());
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptChainHash"] =
        json!(chain_hash.clone());
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewVersionReceiptHashes"] =
        json!([receipt_hash_v2.clone()]);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"] = json!([]);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["verifiedChainCount"] =
        json!(1);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["receiptHashes"] =
        json!([receipt_hash_v1.clone(), receipt_hash_v2.clone()]);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chainHashes"] =
        json!([chain_hash.clone()]);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"] = json!([{
        "stableId": "lakecat:view:local:default:active_customers_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "active_customers_view",
        "chainHash": chain_hash,
        "chainVerified": true,
        "latestViewVersion": 2,
        "latestOperation": "upsert",
        "tombstoned": false,
        "receiptCount": 2,
        "receipts": [receipt_v1, receipt_v2]
    }]);
}

pub(crate) fn qglake_add_other_view_receipt_chain(
    summary: &mut Value,
    chain_hash: String,
    receipt: Value,
    receipt_hash: String,
) {
    let receipt_chain_group =
        &mut summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0];
    receipt_chain_group["chainHashes"]
        .as_array_mut()
        .expect("fixture chainHashes")
        .push(json!(chain_hash.clone()));
    receipt_chain_group["receiptHashes"]
        .as_array_mut()
        .expect("fixture receiptHashes")
        .push(json!(receipt_hash.clone()));
    let chains = receipt_chain_group["chains"]
        .as_array_mut()
        .expect("fixture chains");
    chains.push(json!({
        "stableId": "lakecat:view:local:default:other_view",
        "warehouse": "local",
        "namespace": ["default"],
        "name": "other_view",
        "chainHash": chain_hash,
        "chainVerified": true,
        "latestViewVersion": 1,
        "latestOperation": "upsert",
        "tombstoned": false,
        "receiptCount": 1,
        "receipts": [receipt]
    }));
    receipt_chain_group["verifiedChainCount"] = json!(chains.len());
}

pub(crate) fn qglake_manifest_plan_tasks() -> Vec<Value> {
    vec![serde_json::json!({
        "task-type": "manifest-list",
        "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro"
    })]
}

pub(crate) fn qglake_manifest_child_plan_tasks() -> Vec<Value> {
    vec![serde_json::json!({
        "task-type": "manifest",
        "manifest-list": "file:///tmp/lakecat-qglake/events/metadata/snap-42.avro",
        "manifest-path": "file:///tmp/lakecat-qglake/events/metadata/manifest-42.avro",
        "content": "data"
    })]
}

pub(crate) fn qglake_file_scan_task_with_delete_ref() -> Value {
    serde_json::json!({
        "data-file": {
            "file-path": "file:///tmp/lakecat-qglake/events/data/part-1.parquet"
        },
        "delete-file-references": [0]
    })
}

pub(crate) fn qglake_delete_files() -> Vec<Value> {
    vec![serde_json::json!({
        "content": "position-deletes",
        "file-path": "file:///tmp/lakecat-qglake/events/delete/pos-delete-1.parquet"
    })]
}

pub(crate) fn qglake_view_lineage_verification() -> QueryGraphBootstrapVerification {
    let mut verification = qglake_lineage_verification();
    verification.view_count = 1;
    verification.verified_views = vec!["lakecat:view:local:default:active_customers".to_string()];
    verification.verified_view_versions =
        BTreeMap::from([("lakecat:view:local:default:active_customers".to_string(), 2)]);
    verification.verified_view_receipt_hashes = BTreeMap::from([(
        "lakecat:view:local:default:active_customers".to_string(),
        qglake_fixture_hash("view-version-receipt"),
    )]);
    verification.verified_view_receipt_chain_hashes = BTreeMap::from([(
        "lakecat:view:local:default:active_customers".to_string(),
        qglake_fixture_hash("view-receipt-chain"),
    )]);
    verification
}

pub(crate) fn qglake_lineage_verification() -> QueryGraphBootstrapVerification {
    QueryGraphBootstrapVerification {
        warehouse: "local".to_string(),
        table_count: 1,
        view_count: 0,
        verified_tables: vec!["local.default.events".to_string()],
        verified_views: Vec::new(),
        verified_view_versions: BTreeMap::new(),
        verified_view_receipt_hashes: BTreeMap::new(),
        verified_view_receipt_chain_hashes: BTreeMap::new(),
        bundle_hash: qglake_fixture_hash("bundle"),
        graph_hash: qglake_fixture_hash("graph"),
        open_lineage_hash: qglake_fixture_hash("openlineage"),
        querygraph_import_hash: qglake_fixture_hash("querygraph-import"),
        standards: vec![
            "Iceberg REST".to_string(),
            "Croissant".to_string(),
            "CDIF".to_string(),
            "OSI handoff".to_string(),
            "ODRL".to_string(),
            "Grust catalog graph".to_string(),
            "OpenLineage".to_string(),
        ],
    }
}

pub(crate) fn qglake_table_commit_record_summary() -> lakecat_api::TableCommitRecordResponse {
    lakecat_api::TableCommitRecordResponse {
        warehouse: "local".to_string(),
        namespace: vec!["default".to_string()],
        table: "events".to_string(),
        previous_metadata_location: Some(
            "file:///tmp/lakecat-qglake/events/metadata/00000.json".to_string(),
        ),
        new_metadata_location: Some(
            "file:///tmp/lakecat-qglake/events/metadata/00000.json".to_string(),
        ),
        sequence_number: 1,
        format_version: Some(3),
        snapshot_id: Some(42),
        policy_hash: None,
        request_hash: qglake_fixture_hash("commit-request"),
        response_hash: qglake_fixture_hash("commit-response"),
        idempotency_key_sha256: Some(qglake_fixture_hash("commit-idempotency")),
        commit_hash: qglake_fixture_hash("commit-record"),
        principal_subject: "did:example:agent".to_string(),
        principal_kind: "agent".to_string(),
        committed_at: "2026-06-19T00:00:00Z".to_string(),
    }
}

pub(crate) fn qglake_bootstrap_lineage_summary() -> LineageDrainEventSummary {
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
        policy_ids: vec!["agent-columns".to_string()],
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
        replay_event_hashes: vec![qglake_fixture_hash("bootstrap-replay")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("bootstrap-openlineage")],
    }
}

pub(crate) fn qglake_bootstrap_lineage_summary_for(
    verification: &QueryGraphBootstrapVerification,
    policy_binding_count: usize,
) -> LineageDrainEventSummary {
    let mut summary = qglake_bootstrap_lineage_summary();
    summary.bundle_hash = Some(verification.bundle_hash.clone());
    summary.graph_hash = Some(verification.graph_hash.clone());
    summary.open_lineage_hash = Some(verification.open_lineage_hash.clone());
    summary.querygraph_import_hash = Some(verification.querygraph_import_hash.clone());
    summary.table_artifact_count = verification.table_count;
    summary.view_artifact_count = verification.view_count;
    summary.view_version_receipt_hashes = verification
        .verified_view_receipt_hashes
        .values()
        .cloned()
        .collect();
    summary.policy_binding_count = policy_binding_count;
    summary.standards = verification.standards.clone();
    summary
}

pub(crate) fn qglake_catalog_config_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-config-read".to_string(),
        event_type: "catalog.config-read".to_string(),
        catalog_config_defaults: CatalogConfigResponse::default().defaults,
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: CatalogConfigResponse::default().endpoints,
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("config-authorization")),
        authorization_receipt_action: Some("catalog-config".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 2,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        management_scope_warehouse: Some("local".to_string()),
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("catalog-config-replay")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("catalog-config-openlineage")],
    }
}

pub(crate) fn qglake_view_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-view".to_string(),
        event_type: "view.upserted".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("view-authorization")),
        authorization_receipt_action: Some("view-manage".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 2,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
        view_artifact_count: 0,
        view_version_receipt_hashes: Vec::new(),
        view_version_receipt_chain_hashes: Vec::new(),
        view_version_receipt_chain_verified_count: 0,
        view_version_receipt_chains: Vec::new(),
        view_warehouse: Some("local".to_string()),
        view_namespace: vec!["default".to_string()],
        view_name: Some("active_customers".to_string()),
        view_stable_id: Some("lakecat:view:local:default:active_customers".to_string()),
        view_version: Some(2),
        expected_view_version: Some(1),
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
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("view-replay-event")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("view-replay-openlineage")],
    }
}

pub(crate) fn qglake_view_drop_lineage_summary() -> LineageDrainEventSummary {
    let mut summary = qglake_view_lineage_summary();
    summary.event_id = "evt-view-drop".to_string();
    summary.event_type = "view.dropped".to_string();
    summary.authorization_receipt_action = Some("view-drop".to_string());
    summary.expected_view_version = Some(2);
    summary.replay_event_hashes = vec![qglake_fixture_hash("view-drop-replay-event")];
    summary.replay_open_lineage_hashes = vec![qglake_fixture_hash("view-drop-replay-openlineage")];
    summary
}

pub(crate) fn qglake_view_tombstone_receipt_lineage_summary() -> LineageDrainEventSummary {
    let mut summary = qglake_view_lineage_summary();
    summary.event_id = "evt-view-receipts".to_string();
    summary.event_type = "view.version-receipts-listed".to_string();
    summary.authorization_receipt_action = Some("view-load".to_string());
    summary.graph_events = 0;
    summary.lineage_events = 1;
    summary.expected_view_version = None;
    summary.view_version_receipt_hashes = vec![qglake_fixture_hash("view-drop-receipt")];
    summary.replay_event_hashes = vec![qglake_fixture_hash("view-receipts-replay-event")];
    summary.replay_open_lineage_hashes =
        vec![qglake_fixture_hash("view-receipts-replay-openlineage")];
    summary
}

pub(crate) fn qglake_view_receipt_chain_lineage_summary() -> LineageDrainEventSummary {
    let mut summary = qglake_view_lineage_summary();
    summary.event_id = "evt-view-receipt-chains".to_string();
    summary.event_type = "view.version-receipt-chains-listed".to_string();
    summary.authorization_receipt_action = Some("view-load".to_string());
    summary.graph_events = 0;
    summary.lineage_events = 1;
    summary.view_stable_id = None;
    summary.view_warehouse = Some("local".to_string());
    summary.view_namespace = vec!["default".to_string()];
    summary.view_name = None;
    summary.view_version = None;
    summary.expected_view_version = None;
    summary.view_version_receipt_hashes = vec![qglake_fixture_hash("view-drop-receipt")];
    summary.view_version_receipt_chain_hashes = vec![qglake_fixture_hash("view-receipt-chain")];
    summary.view_version_receipt_chain_verified_count = 1;
    summary.view_version_receipt_chains = vec![ViewVersionReceiptChainResponse {
        stable_id: "lakecat:view:local:default:active_customers".to_string(),
        warehouse: "local".to_string(),
        namespace: vec!["default".to_string()],
        name: "active_customers".to_string(),
        chain_hash: qglake_fixture_hash("view-receipt-chain"),
        chain_verified: true,
        latest_view_version: 2,
        latest_operation: "drop".to_string(),
        tombstoned: true,
        receipt_count: 3,
        receipts: vec![
            ViewVersionReceiptResponse {
                stable_id: "lakecat:view:local:default:active_customers".to_string(),
                warehouse: "local".to_string(),
                namespace: vec!["default".to_string()],
                name: "active_customers".to_string(),
                view_version: 1,
                previous_view_version: None,
                previous_receipt_hash: None,
                operation: "upsert".to_string(),
                view_hash: qglake_fixture_hash("view-v1"),
                receipt_hash: qglake_fixture_hash("view-receipt-v1"),
                principal_subject: "did:example:agent".to_string(),
                principal_kind: "agent".to_string(),
                recorded_at: "2026-06-20T00:00:00Z".to_string(),
            },
            ViewVersionReceiptResponse {
                stable_id: "lakecat:view:local:default:active_customers".to_string(),
                warehouse: "local".to_string(),
                namespace: vec!["default".to_string()],
                name: "active_customers".to_string(),
                view_version: 2,
                previous_view_version: Some(1),
                previous_receipt_hash: Some(qglake_fixture_hash("view-receipt-v1")),
                operation: "upsert".to_string(),
                view_hash: qglake_fixture_hash("view-v2"),
                receipt_hash: qglake_fixture_hash("view-version-receipt"),
                principal_subject: "did:example:agent".to_string(),
                principal_kind: "agent".to_string(),
                recorded_at: "2026-06-20T00:00:01Z".to_string(),
            },
            ViewVersionReceiptResponse {
                stable_id: "lakecat:view:local:default:active_customers".to_string(),
                warehouse: "local".to_string(),
                namespace: vec!["default".to_string()],
                name: "active_customers".to_string(),
                view_version: 2,
                previous_view_version: Some(2),
                previous_receipt_hash: Some(qglake_fixture_hash("view-version-receipt")),
                operation: "drop".to_string(),
                view_hash: qglake_fixture_hash("view-drop"),
                receipt_hash: qglake_fixture_hash("view-drop-receipt"),
                principal_subject: "did:example:agent".to_string(),
                principal_kind: "agent".to_string(),
                recorded_at: "2026-06-20T00:00:02Z".to_string(),
            },
        ],
    }];
    summary.replay_event_hashes = vec![qglake_fixture_hash("view-receipt-chains-replay-event")];
    summary.replay_open_lineage_hashes = vec![qglake_fixture_hash(
        "view-receipt-chains-replay-openlineage",
    )];
    summary
}

pub(crate) fn qglake_table_commit_history_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-table-commits".to_string(),
        event_type: "table.commits-listed".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("table-commits-authorization")),
        authorization_receipt_action: Some("table-load".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 1,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        table_commit_count: Some(1),
        table_commit_sequence_numbers: vec![1],
        table_commit_hashes: vec![qglake_fixture_hash("table-commit")],
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
        management_scope_warehouse: Some("local".to_string()),
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("table-commits-replay-event")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("table-commits-openlineage")],
    }
}

pub(crate) fn qglake_read_restriction_summary() -> Value {
    json!({
        "allowed-columns": ["event_id", "occurred_at", "severity"],
        "row-predicate": {
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        },
        "purpose": "qglake-agent-demo",
        "max-credential-ttl-seconds": 300,
        "policy-hashes": [qglake_fixture_hash("scan-policy")]
    })
}

pub(crate) fn qglake_fixture_hash(label: &str) -> String {
    content_hash_bytes(label.as_bytes())
}

pub(crate) fn qglake_scan_planned_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-scan-planned".to_string(),
        event_type: "table.scan-planned".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("scan-planned-authorization")),
        authorization_receipt_action: Some("table-plan-scan".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 1,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        scan_task_count: Some(1),
        file_scan_task_count: None,
        delete_file_count: None,
        child_plan_task_count: None,
        read_restriction: Some(qglake_read_restriction_summary()),
        required_projection: Vec::new(),
        requested_projection: vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
            "raw_payload".to_string(),
        ],
        effective_projection: vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
        ],
        required_filters: Vec::new(),
        requested_stats_fields: vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
            "raw_payload".to_string(),
        ],
        effective_stats_fields: vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
        ],
        management_scope_project_id: None,
        management_scope_warehouse: Some("local".to_string()),
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("scan-planned-replay")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("scan-planned-openlineage")],
    }
}

pub(crate) fn qglake_scan_tasks_fetched_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-scan-tasks-fetched".to_string(),
        event_type: "table.scan-tasks-fetched".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("scan-fetch-authorization")),
        authorization_receipt_action: Some("table-plan-scan".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 0,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        file_scan_task_count: Some(1),
        delete_file_count: Some(1),
        child_plan_task_count: Some(1),
        read_restriction: Some(qglake_read_restriction_summary()),
        required_projection: vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
        ],
        requested_projection: Vec::new(),
        effective_projection: vec![
            "event_id".to_string(),
            "occurred_at".to_string(),
            "severity".to_string(),
        ],
        required_filters: vec![json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        })],
        requested_stats_fields: Vec::new(),
        effective_stats_fields: Vec::new(),
        management_scope_project_id: None,
        management_scope_warehouse: Some("local".to_string()),
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("scan-fetch-replay")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("scan-fetch-openlineage")],
    }
}

pub(crate) fn qglake_policy_list_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-policy-list".to_string(),
        event_type: "policy-binding.listed".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("policy-list-authorization")),
        authorization_receipt_action: Some("policy-manage".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 1,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        policy_ids: vec!["agent-columns".to_string()],
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
        management_scope_warehouse: Some("local".to_string()),
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("policy-list-replay-event")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("policy-list-openlineage")],
    }
}

pub(crate) fn qglake_policy_upsert_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-policy-upsert".to_string(),
        event_type: "policy-binding.upserted".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("policy-upsert-authorization")),
        authorization_receipt_action: Some("policy-manage".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 1,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        policy_id: Some("agent-columns".to_string()),
        policy_odrl_hash: Some(qglake_fixture_hash("policy-odrl")),
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
        management_scope_warehouse: Some("local".to_string()),
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("policy-upsert-replay")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("policy-upsert-openlineage")],
    }
}

pub(crate) fn qglake_storage_profile_list_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-storage-profile-list".to_string(),
        event_type: "storage-profile.listed".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("storage-profile-list-authorization")),
        authorization_receipt_action: Some("storage-profile-manage".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 1,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        storage_profile_count: Some(1),
        storage_profile_ids: vec!["events-local".to_string()],
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
        management_scope_warehouse: Some("local".to_string()),
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("storage-profile-list-replay-event")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("storage-profile-list-openlineage")],
    }
}

pub(crate) fn qglake_storage_profile_upsert_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-storage-profile-upsert".to_string(),
        event_type: "storage-profile.upserted".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash(
            "storage-profile-upsert-authorization",
        )),
        authorization_receipt_action: Some("storage-profile-manage".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 1,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        storage_profile_id: Some("events-local".to_string()),
        storage_profile_provider: Some("file".to_string()),
        storage_profile_issuance_mode: Some("local-file-no-secret".to_string()),
        storage_profile_location_prefix_hash: Some(
            "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
        ),
        storage_profile_secret_ref_present: Some(false),
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
        management_scope_warehouse: Some("local".to_string()),
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("storage-profile-upsert-replay-event")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("storage-profile-upsert-openlineage")],
    }
}

pub(crate) fn qglake_server_list_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-server-list".to_string(),
        event_type: "server.listed".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("server-list-authorization")),
        authorization_receipt_action: Some("server-manage".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 1,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        server_count: Some(1),
        server_ids: vec!["qglake-server".to_string()],
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
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("server-list-replay-event")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("server-list-openlineage")],
    }
}

pub(crate) fn qglake_project_list_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-project-list".to_string(),
        event_type: "project.listed".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("project-list-authorization")),
        authorization_receipt_action: Some("project-manage".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 1,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        project_count: Some(1),
        project_ids: vec!["analytics".to_string()],
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
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("project-list-replay-event")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("project-list-openlineage")],
    }
}

pub(crate) fn qglake_warehouse_list_lineage_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-warehouse-list".to_string(),
        event_type: "warehouse.listed".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("warehouse-list-authorization")),
        authorization_receipt_action: Some("warehouse-manage".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 1,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        warehouse_count: Some(1),
        warehouse_names: vec!["local".to_string()],
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
        management_scope_project_id: Some("analytics".to_string()),
        management_scope_warehouse: None,
        standards: Vec::new(),
        credential_count: None,
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: None,
        raw_credential_exception_allowed: None,
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("warehouse-list-replay-event")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("warehouse-list-openlineage")],
    }
}

pub(crate) fn qglake_restricted_credential_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-agent-credentials".to_string(),
        event_type: "credentials.vend-attempted".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("did:example:agent".to_string()),
        principal_kind: Some("agent".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash(
            "restricted-credential-authorization",
        )),
        authorization_receipt_action: Some("credentials-vend".to_string()),
        request_identity_state: Some("verified".to_string()),
        request_identity_source: Some("x-lakecat-agent-did".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: Some(qglake_fixture_hash("delegation")),
        agent_summary_signature_hash: Some(qglake_fixture_hash("summary")),
        graph_events: 2,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        storage_profile_id: Some("events-local".to_string()),
        storage_profile_provider: Some("file".to_string()),
        storage_profile_issuance_mode: Some("local-file-no-secret".to_string()),
        storage_profile_location_prefix_hash: Some(
            "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
        ),
        storage_profile_secret_ref_present: Some(false),
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
        read_restriction: Some(qglake_read_restriction_summary()),
        required_projection: Vec::new(),
        requested_projection: Vec::new(),
        effective_projection: Vec::new(),
        required_filters: Vec::new(),
        requested_stats_fields: Vec::new(),
        effective_stats_fields: Vec::new(),
        management_scope_project_id: None,
        management_scope_warehouse: None,
        standards: Vec::new(),
        credential_count: Some(0),
        credential_prefix_hashes: Vec::new(),
        credential_block_reason: Some(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON.to_string()),
        raw_credential_exception_allowed: Some(false),
        raw_credential_exception_reason: None,
        replay_event_hashes: vec![qglake_fixture_hash("restricted-credential-replay")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("restricted-credential-openlineage")],
    }
}

pub(crate) fn qglake_human_credential_summary() -> LineageDrainEventSummary {
    LineageDrainEventSummary {
        event_id: "evt-human-credentials".to_string(),
        event_type: "credentials.vend-attempted".to_string(),
        catalog_config_defaults: Vec::new(),
        catalog_config_overrides: Vec::new(),
        catalog_config_endpoints: Vec::new(),
        principal_subject: Some("human:qglake-operator".to_string()),
        principal_kind: Some("human".to_string()),
        authorization_receipt_hash: Some(qglake_fixture_hash("human-credential-authorization")),
        authorization_receipt_action: Some("credentials-vend".to_string()),
        request_identity_state: Some("header-principal".to_string()),
        request_identity_source: Some("x-lakecat-principal".to_string()),
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        agent_delegation_hash: None,
        agent_summary_signature_hash: None,
        graph_events: 2,
        lineage_events: 1,
        bundle_hash: None,
        graph_hash: None,
        open_lineage_hash: None,
        querygraph_import_hash: None,
        table_artifact_count: 0,
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
        storage_profile_id: Some("events-local".to_string()),
        storage_profile_provider: Some("file".to_string()),
        storage_profile_issuance_mode: Some("local-file-no-secret".to_string()),
        storage_profile_location_prefix_hash: Some(
            "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
        ),
        storage_profile_secret_ref_present: Some(false),
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
        read_restriction: Some(qglake_read_restriction_summary()),
        required_projection: Vec::new(),
        requested_projection: Vec::new(),
        effective_projection: Vec::new(),
        required_filters: Vec::new(),
        requested_stats_fields: Vec::new(),
        effective_stats_fields: Vec::new(),
        management_scope_project_id: None,
        management_scope_warehouse: None,
        standards: Vec::new(),
        credential_count: Some(1),
        credential_prefix_hashes: vec![qglake_fixture_hash("human-credential-prefix")],
        credential_block_reason: None,
        raw_credential_exception_allowed: Some(true),
        raw_credential_exception_reason: Some(
            QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON.to_string(),
        ),
        replay_event_hashes: vec![qglake_fixture_hash("human-credential-replay")],
        replay_open_lineage_hashes: vec![qglake_fixture_hash("human-credential-openlineage")],
    }
}

pub(crate) fn qglake_lineage_standards() -> Vec<String> {
    vec![
        "Iceberg REST".to_string(),
        "Croissant".to_string(),
        "CDIF".to_string(),
        "OSI handoff".to_string(),
        "ODRL".to_string(),
        "Grust catalog graph".to_string(),
        "OpenLineage".to_string(),
    ]
}

pub(crate) fn qglake_querygraph_projection(
    policy: serde_json::Value,
) -> lakecat_querygraph::QueryGraphTableProjection {
    qglake_querygraph_projection_for("events", policy)
}

pub(crate) fn qglake_querygraph_projection_for(
    table: &str,
    policy: serde_json::Value,
) -> lakecat_querygraph::QueryGraphTableProjection {
    let warehouse = lakecat_core::WarehouseName::new("local").unwrap();
    let namespace = lakecat_core::Namespace::new(vec!["default".to_string()]).unwrap();
    let table_name = lakecat_core::TableName::new(table).unwrap();
    let ident = lakecat_core::TableIdent::new(warehouse, namespace, table_name);
    let stable_id = ident.stable_id();
    lakecat_querygraph::QueryGraphTableProjection {
        ident,
        stable_id: stable_id.clone(),
        location: format!("file:///tmp/lakecat-qglake/{table}"),
        metadata_location: Some(format!(
            "file:///tmp/lakecat-qglake/{table}/metadata/00000.json"
        )),
        version: 0,
        format_version: Some(3),
        croissant: serde_json::json!({}),
        cdif: serde_json::json!({}),
        osi: serde_json::json!({}),
        odrl: serde_json::json!({
            "lakecat:policy-bindings": [{
                "policy-id": "events-agent-read",
                "odrl": policy.clone()
            }]
        }),
        policy_bindings: vec![lakecat_querygraph::QueryGraphPolicyBindingProjection {
            policy_id: "events-agent-read".to_string(),
            enforced: true,
            namespace: Some(vec!["default".to_string()]),
            table: Some("events".to_string()),
            odrl: policy,
        }],
    }
}

pub(crate) fn qglake_querygraph_bundle(
    tables: Vec<lakecat_querygraph::QueryGraphTableProjection>,
    open_lineage_outputs: Vec<serde_json::Value>,
) -> QueryGraphBootstrap {
    let table_count = tables.len();
    let table_artifacts = tables
        .iter()
        .map(|table| lakecat_querygraph::QueryGraphTableArtifactHashes {
            stable_id: table.stable_id.clone(),
            croissant_hash: content_hash_json(&table.croissant).unwrap(),
            cdif_hash: content_hash_json(&table.cdif).unwrap(),
            osi_hash: content_hash_json(&table.osi).unwrap(),
            odrl_hash: content_hash_json(&table.odrl).unwrap(),
            policy_bindings_hash: content_hash_json(
                &serde_json::to_value(&table.policy_bindings).unwrap(),
            )
            .unwrap(),
        })
        .collect::<Vec<_>>();
    let open_lineage_outputs = open_lineage_outputs
        .into_iter()
        .map(|mut output| {
            if output.pointer("/facets/dataSource/uri").is_none() {
                output["facets"]["dataSource"] = serde_json::json!({
                    "uri": "file:///tmp/lakecat-qglake/events"
                });
            }
            output
        })
        .collect::<Vec<_>>();
    let graph = lakecat_querygraph::QueryGraphCatalogGraph::from_tables(&tables);
    let graph_hash = content_hash_json(&serde_json::to_value(&graph).unwrap()).unwrap();
    let open_lineage = serde_json::json!({
        "eventType": "COMPLETE",
        "job": {
            "namespace": "lakecat.local",
            "name": "querygraph-bootstrap"
        },
        "producer": "https://querygraph.ai/lakecat",
        "schemaURL": "https://openlineage.io/spec/2-0-2/OpenLineage.json",
        "run": {
            "facets": {
                "queryGraph_semanticBundle": {
                    "tableCount": table_count,
                    "viewCount": 0,
                    "standards": [
                        "Iceberg REST",
                        "Croissant",
                        "CDIF",
                        "OSI handoff",
                        "ODRL",
                        "Grust catalog graph",
                        "OpenLineage"
                    ],
                    "graphHash": graph_hash,
                    "tableArtifacts": table_artifacts.iter().map(qglake_open_lineage_table_artifact).collect::<Vec<_>>(),
                    "viewArtifacts": []
                }
            }
        },
        "outputs": open_lineage_outputs
    });
    let mut manifest = lakecat_querygraph::QueryGraphBundleManifest {
        schema_version: "lakecat.querygraph.bootstrap.v1".to_string(),
        producer: "https://querygraph.ai/lakecat".to_string(),
        standards: vec![
            "Iceberg REST".to_string(),
            "Croissant".to_string(),
            "CDIF".to_string(),
            "OSI handoff".to_string(),
            "ODRL".to_string(),
            "Grust catalog graph".to_string(),
            "OpenLineage".to_string(),
        ],
        table_artifacts,
        view_artifacts: Vec::new(),
        graph_hash,
        open_lineage_hash: content_hash_json(&open_lineage).unwrap(),
        querygraph_import: None,
    };
    let warehouse = lakecat_core::WarehouseName::new("local").unwrap();
    manifest.querygraph_import = Some(lakecat_querygraph::QueryGraphImportCompatibility {
        schema_version: "lakecat.querygraph.import-compat.v1".to_string(),
        table_only_bundle_hash: qglake_querygraph_import_hash(
            &warehouse,
            &manifest,
            &tables,
            &graph,
            &open_lineage,
        ),
        view_count: 0,
        graph_hash: manifest.graph_hash.clone(),
        view_receipt_evidence: Vec::new(),
        view_receipt_evidence_hash: None,
    });
    let bundle_hash = content_hash_json(&serde_json::json!({
        "warehouse": warehouse.as_str(),
        "manifest": &manifest,
        "tables": &tables,
        "views": Vec::<serde_json::Value>::new(),
        "graph": &graph,
        "openLineage": &open_lineage,
    }))
    .unwrap();
    QueryGraphBootstrap {
        warehouse,
        generated_at: chrono::Utc::now(),
        bundle_hash,
        manifest,
        tables,
        views: Vec::new(),
        graph,
        open_lineage,
    }
}

pub(crate) fn qglake_querygraph_import_hash(
    warehouse: &lakecat_core::WarehouseName,
    manifest: &lakecat_querygraph::QueryGraphBundleManifest,
    tables: &[lakecat_querygraph::QueryGraphTableProjection],
    graph: &lakecat_querygraph::QueryGraphCatalogGraph,
    open_lineage: &serde_json::Value,
) -> String {
    content_hash_json(&serde_json::json!({
        "warehouse": warehouse.as_str(),
        "manifest": {
            "schema-version": manifest.schema_version,
            "producer": manifest.producer,
            "standards": manifest.standards,
            "table-artifacts": manifest.table_artifacts.iter().map(|artifact| serde_json::json!({
                "stable-id": artifact.stable_id,
                "croissant-hash": artifact.croissant_hash,
                "cdif-hash": artifact.cdif_hash,
                "osi-hash": artifact.osi_hash,
                "odrl-hash": artifact.odrl_hash,
            })).collect::<Vec<_>>(),
            "open-lineage-hash": manifest.open_lineage_hash,
        },
        "tables": tables.iter().map(|table| serde_json::json!({
            "ident": table.ident,
            "stable-id": table.stable_id,
            "location": table.location,
            "metadata-location": table.metadata_location,
            "version": table.version,
            "format-version": table.format_version,
            "croissant": table.croissant,
            "cdif": table.cdif,
            "osi": table.osi,
            "odrl": table.odrl,
        })).collect::<Vec<_>>(),
        "graph": graph,
        "openLineage": open_lineage,
    }))
    .unwrap()
}

pub(crate) fn qglake_open_lineage_table_artifact(
    artifact: &lakecat_querygraph::QueryGraphTableArtifactHashes,
) -> Value {
    serde_json::json!({
        "stableId": artifact.stable_id,
        "croissantHash": artifact.croissant_hash,
        "cdifHash": artifact.cdif_hash,
        "osiHash": artifact.osi_hash,
        "odrlHash": artifact.odrl_hash,
        "policyBindingsHash": artifact.policy_bindings_hash
    })
}
