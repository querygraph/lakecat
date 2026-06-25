use super::common::*;
use crate::*;

#[test]
fn qglake_handoff_summary_verifier_rejects_scan_stats_field_widening() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedEffectiveStatsFields"] =
        json!(["event_id", "occurred_at", "severity", "raw_payload"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject widened scan stats fields");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(
        err.to_string()
            .contains("must prove a wider request than plannedEffectiveStatsFields")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_unrequested_effective_scan_stats_field() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedRequestedStatsFields"] =
        json!(["event_id", "occurred_at", "severity", "raw_payload"]);
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedEffectiveStatsFields"] =
        json!(["event_id", "occurred_at", "tenant_id"]);

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject effective stats fields that were never requested",
    );

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedEffectiveStatsFields"));
    assert!(err.to_string().contains("tenant_id"));
    assert!(err.to_string().contains("not requested"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_blank_requested_scan_stats_field() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedRequestedStatsFields"] =
        json!(["event_id", "occurred_at", "severity", "raw_payload", " "]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject blank requested scan stats field");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedRequestedStatsFields"));
    assert!(err.to_string().contains("non-empty"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_scan_restriction_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedReadRestriction"]["allowed-columns"] =
        json!(["event_id", "raw_payload"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject drifted scan restriction");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("allowed-columns mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_empty_scan_allowed_columns() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]["allowed-columns"] =
        json!(["event_id", ""]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject empty scan allowed columns");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("allowed-columns"));
    assert!(err.to_string().contains("column names"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_scan_allowed_columns() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]["allowed-columns"] =
        json!(["event_id", "event_id"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate scan allowed columns");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("allowed-columns"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_scan_restriction_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]["unverifiedRestrictionClaim"] =
        json!(qglake_fixture_hash("unverified-scan-restriction-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra scan restriction fields");
    let err = err.to_string();

    assert!(
        err.contains("governedScanProof.plannedReadRestriction"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedRestrictionClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_empty_scan_row_predicate() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]["row-predicate"] =
        json!({});

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject empty scan row predicate");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("row-predicate"));
    assert!(err.to_string().contains("predicate evidence"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_blank_scan_row_predicate_type() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]["row-predicate"] = json!({
        "type": " ",
        "term": "severity",
        "value": "debug"
    });

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject blank scan row predicate type");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("row-predicate"));
    assert!(err.to_string().contains("type"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_scan_row_predicate_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]["row-predicate"]
        ["unverifiedPredicateClaim"] =
        json!(qglake_fixture_hash("unverified-scan-predicate-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra scan row-predicate fields");
    let err = err.to_string();

    assert!(
        err.contains("governedScanProof.plannedReadRestriction.row-predicate"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedPredicateClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_termless_scan_row_predicate() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]["row-predicate"] = json!({
        "type": "not-eq",
        "value": "debug"
    });

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject termless scan row predicate");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("row-predicate"));
    assert!(err.to_string().contains("term"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_scan_restriction_purpose() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]
        .as_object_mut()
        .unwrap()
        .remove("purpose");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing scan restriction purpose");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("purpose"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_scan_restriction_purpose_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedReadRestriction"]["purpose"] =
        json!("different-purpose");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject drifted scan restriction purpose");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("purpose mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_scan_restriction_ttl_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedReadRestriction"]["max-credential-ttl-seconds"] =
        json!(60);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject drifted scan restriction TTL cap");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(
        err.to_string()
            .contains("max-credential-ttl-seconds mismatch")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_scan_policy_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReadRestriction"]["policy-hashes"] =
        json!(["sha256:scan-policy"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short scan policy hashes");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("policy-hashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_fetch_requirement_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("fetchedRequiredProjection");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing fetch requirement evidence");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("fetchedRequiredProjection"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_fetch_effective_projection_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("fetchedEffectiveProjection");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing fetch effective projection evidence");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("fetchedEffectiveProjection"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_fetch_filter_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]
        .as_object_mut()
        .unwrap()
        .remove("fetchedRequiredFilters");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing fetched filter evidence");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("fetchedRequiredFilters"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_fetch_filter_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedRequiredFilters"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "type": "eq",
            "term": "tenant_id",
            "value": "other"
        }));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra fetched filter evidence");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("fetchedRequiredFilters"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_scan_openlineage_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["fetchedOpenLineageHashes"] =
        json!([]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing scan OpenLineage hashes");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("fetchedOpenLineageHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_scan_replay_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedReplayEventHashes"] =
        json!(["sha256:scan-plan-replay"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short governed scan replay hashes");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedReplayEventHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_scan_openlineage_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_hash = qglake_fixture_hash("duplicate-scan-plan-openlineage");
    summary["lakecatReplayVerification"]["governedScanProof"]["plannedOpenLineageHashes"] =
        json!([duplicate_hash, duplicate_hash]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate scan OpenLineage hashes");

    assert!(err.to_string().contains("governedScanProof"));
    assert!(err.to_string().contains("plannedOpenLineageHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_table_commit_history_count_match() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitCount"] = json!(2);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject commit-history count drift");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("sequenceNumbers"));
    assert!(err.to_string().contains("length mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_accepts_empty_table_commit_history() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitCount"] = json!(0);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["sequenceNumbers"] = json!([]);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitHashes"] = json!([]);

    verify_qglake_handoff_summary_value(&summary)
        .expect("handoff summary should accept explicit zero-count commit-history proof");
}

#[test]
fn qglake_handoff_summary_verifier_requires_commit_history_principal() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]
        .as_object_mut()
        .unwrap()
        .remove("principalSubject");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing commit-history principal proof");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("principalSubject"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_commit_history_principal_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["principalSubject"] =
        json!("did:example:other");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject commit-history principal drift");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("principalSubject mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_commit_history_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["unverifiedCommitClaim"] =
        json!(qglake_fixture_hash("unverified-commit-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unverified commit-history fields");
    let err = err.to_string();

    assert!(err.contains("tableCommitHistoryProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedCommitClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_commit_history_authorization_hash() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["authorizationReceiptHash"] =
        json!("sha256:table-commits");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short commit-history authorization hash");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("authorizationReceiptHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_commit_history_authorization_action() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["authorizationReceiptAction"] =
        json!("table-commit");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject wrong commit-history authorization action");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("authorizationReceiptAction"));
    assert!(err.to_string().contains("table-load"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_positive_commit_sequences() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["sequenceNumbers"] = json!([0]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject non-positive commit sequences");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("sequenceNumbers"));
    assert!(err.to_string().contains("positive"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_increasing_commit_sequences() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitCount"] = json!(2);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["sequenceNumbers"] =
        json!([1, 1]);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitHashes"] =
        json!(["sha256:first", "sha256:second"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate commit sequences");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("sequenceNumbers"));
    assert!(err.to_string().contains("strictly increasing"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_commit_history_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_hash = qglake_fixture_hash("commit-history-duplicate");
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitCount"] = json!(2);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["sequenceNumbers"] =
        json!([1, 2]);
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitHashes"] =
        json!([duplicate_hash, duplicate_hash]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate commit hashes");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("commitHashes"));
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_commit_history_replay_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["replayEventHashes"] =
        json!([]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing commit-history replay hashes");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_commit_history_replay_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_hash = qglake_fixture_hash("duplicate-commit-history-replay");
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["replayEventHashes"] =
        json!([duplicate_hash, duplicate_hash]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate commit-history replay hashes");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_commit_history_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["commitHashes"] =
        json!(["sha256:commit"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short commit-history hashes");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("commitHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_commit_history_graph_events() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["tableCommitHistoryProof"]["graphEvents"] = json!(0);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing commit-history graph projection");

    assert!(err.to_string().contains("tableCommitHistoryProof"));
    assert!(err.to_string().contains("graphEvents"));
    assert!(err.to_string().contains("positive"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_restricted_credential_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["replayEventHashes"] =
        json!([]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing restricted credential replay hashes");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_credential_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["unverifiedCredentialClaim"] =
        json!(qglake_fixture_hash("unverified-credential-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra credential proof fields");
    let err = err.to_string();

    assert!(err.contains("credentialVendingProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedCredentialClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_credential_branch_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["unverifiedRawCredentialClaim"] =
        json!(qglake_fixture_hash("unverified-raw-credential-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra credential branch fields");
    let err = err.to_string();

    assert!(err.contains("credentialVendingProof.trustedHuman"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedRawCredentialClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_credential_storage_profile_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["storageProfile"]
        ["unverifiedStorageScopeClaim"] =
        json!(qglake_fixture_hash("unverified-storage-scope-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra credential storage-profile fields");
    let err = err.to_string();

    assert!(
        err.contains("credentialVendingProof.restricted.storageProfile"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedStorageScopeClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_credential_replay_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["replayEventHashes"] =
        json!(["sha256:restricted-replay"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short credential replay hashes");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("replayEventHashes"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_credential_authorization_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]
        .as_object_mut()
        .unwrap()
        .remove("authorizationReceiptHash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing credential authorization hash");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("authorizationReceiptHash"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_credential_authorization_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["authorizationReceiptHash"] =
        json!("sha256:credential-auth");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short credential authorization hash");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("authorizationReceiptHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_credential_authorization_actions() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["authorizationReceiptAction"] =
        json!("table-load");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject credential authorization action drift");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("authorizationReceiptAction"));
    assert!(err.to_string().contains("credentials-vend"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_credential_prefix_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]
        .as_object_mut()
        .unwrap()
        .remove("credentialPrefixHashes");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing trusted-human credential prefix hashes");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("credentialPrefixHashes"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_credential_prefix_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_hash = qglake_fixture_hash("duplicate-human-credential-prefix");
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["credentialCount"] =
        json!(2);
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["credentialPrefixHashes"] =
        json!([duplicate_hash, duplicate_hash]);

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject duplicate trusted-human credential prefix hashes",
    );

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("credentialPrefixHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_credential_openlineage_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_hash = qglake_fixture_hash("duplicate-restricted-openlineage");
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["openLineageHashes"] =
        json!([duplicate_hash, duplicate_hash]);

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject duplicate restricted credential OpenLineage hashes",
    );

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("openLineageHashes"));
    assert!(err.to_string().contains("duplicate-free"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_restricted_raw_exception_flag() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]
        .as_object_mut()
        .unwrap()
        .remove("rawCredentialExceptionAllowed");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should require restricted raw credential exception evidence");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("rawCredentialExceptionAllowed"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_restricted_raw_exception() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["rawCredentialExceptionAllowed"] =
        json!(true);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject restricted raw credential exceptions");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(
        err.to_string()
            .contains("must not allow a raw credential exception")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_restricted_exception_reason() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["rawCredentialExceptionReason"] =
        json!("trusted-human-override");

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject a raw credential exception reason on blocked restricted proofs",
    );

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("rawCredentialExceptionReason"));
    assert!(err.to_string().contains("must be null"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_credential_ttl_cap() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]
        .as_object_mut()
        .unwrap()
        .remove("maxCredentialTtlSeconds");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing credential TTL evidence");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("maxCredentialTtlSeconds"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_credential_ttl_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["maxCredentialTtlSeconds"] =
        json!(60);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject credential TTL drift");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("maxCredentialTtlSeconds mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_credential_storage_profile_graph_evidence() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]
        .as_object_mut()
        .unwrap()
        .remove("storageProfile");

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject credential proof without storage-profile graph evidence",
    );

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("storageProfile"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_credential_location_prefix_hash() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]
        ["storageProfile"]
        .as_object_mut()
        .unwrap()
        .remove("locationPrefixHash");

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject credential proof without storage-scope hash evidence",
    );

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("locationPrefixHash"));

    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["storageProfile"]
        ["locationPrefixHash"] = json!("sha256:storage-location-prefix");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short credential storage-scope hash evidence");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("locationPrefixHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_credential_secret_ref_provider_when_present() {
    let mut summary = qglake_handoff_summary_json();
    let storage_profile = summary["lakecatReplayVerification"]["credentialVendingProof"]
        ["restricted"]["storageProfile"]
        .as_object_mut()
        .unwrap();
    storage_profile.insert("secretRefPresent".to_string(), json!(true));
    storage_profile.insert("secretRefProvider".to_string(), Value::Null);
    storage_profile.insert(
        "secretRefHash".to_string(),
        json!(qglake_fixture_hash("credential-secret-ref")),
    );

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject credential secret-ref presence without provider",
    );

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("secretRefProvider"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_blank_credential_secret_ref_provider_when_present() {
    let mut summary = qglake_handoff_summary_json();
    let storage_profile = summary["lakecatReplayVerification"]["credentialVendingProof"]
        ["trustedHuman"]["storageProfile"]
        .as_object_mut()
        .unwrap();
    storage_profile.insert("secretRefPresent".to_string(), json!(true));
    storage_profile.insert("secretRefProvider".to_string(), json!("\t "));
    storage_profile.insert(
        "secretRefHash".to_string(),
        json!(qglake_fixture_hash("credential-secret-ref")),
    );

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject credential secret-ref presence with blank provider",
    );

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("secretRefProvider"));
    assert!(err.to_string().contains("blank"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_credential_secret_ref_hash_when_present() {
    let mut summary = qglake_handoff_summary_json();
    let storage_profile = summary["lakecatReplayVerification"]["credentialVendingProof"]
        ["trustedHuman"]["storageProfile"]
        .as_object_mut()
        .unwrap();
    storage_profile.insert("secretRefPresent".to_string(), json!(true));
    storage_profile.insert("secretRefProvider".to_string(), json!("vault"));
    storage_profile.insert("secretRefHash".to_string(), Value::Null);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject credential secret-ref presence without hash");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("secretRefHash"));

    let mut summary = qglake_handoff_summary_json();
    let storage_profile = summary["lakecatReplayVerification"]["credentialVendingProof"]
        ["trustedHuman"]["storageProfile"]
        .as_object_mut()
        .unwrap();
    storage_profile.insert("secretRefPresent".to_string(), json!(true));
    storage_profile.insert("secretRefProvider".to_string(), json!("vault"));
    storage_profile.insert("secretRefHash".to_string(), json!("not-a-sha256-hash"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject malformed credential secret-ref hash");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("secretRefHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_credential_secret_ref_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let storage_profile = summary["lakecatReplayVerification"]["credentialVendingProof"]
        ["trustedHuman"]["storageProfile"]
        .as_object_mut()
        .unwrap();
    storage_profile.insert("secretRefPresent".to_string(), json!(true));
    storage_profile.insert("secretRefProvider".to_string(), json!("vault"));
    storage_profile.insert(
        "secretRefHash".to_string(),
        json!("sha256:credential-secret-ref"),
    );

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short credential secret-ref hashes");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("secretRefHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_credential_secret_ref_hash_when_absent() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["storageProfile"]
        ["secretRefHash"] = json!("sha256:credential-secret-ref");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject credential secret-ref hash without presence");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("secretRefHash"));
    assert!(err.to_string().contains("null"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_credential_secret_ref_provider_when_absent() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["storageProfile"]
        ["secretRefProvider"] = json!({"provider": "vault"});

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject credential secret-ref provider evidence without presence",
    );

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("secretRefProvider"));
    assert!(err.to_string().contains("absent or null"));
}

#[test]
fn qglake_handoff_summary_verifier_accepts_secret_ref_backed_credential_proof() {
    let mut summary = qglake_handoff_summary_json();
    let secret_ref_hash = qglake_fixture_hash("qglake-production-secret-ref");
    set_qglake_secret_ref_backed_profile(
        &mut summary["lakecatReplayVerification"]["storageProfileUpsertProof"],
        &secret_ref_hash,
    );
    set_qglake_secret_ref_backed_profile(
        &mut summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["storageProfile"],
        &secret_ref_hash,
    );
    set_qglake_secret_ref_backed_profile(
        &mut summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["storageProfile"],
        &secret_ref_hash,
    );

    verify_qglake_handoff_summary_value(&summary)
        .expect("handoff summary should accept matching redacted secret-ref proof");
}

#[test]
fn qglake_handoff_summary_verifier_rejects_credential_storage_profile_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["restricted"]["storageProfile"]
        ["profileId"] = json!("other-profile");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject credential storage-profile drift");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("profileId"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_credential_secret_ref_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefPresent"] =
        json!(true);
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefProvider"] =
        json!("vault");
    summary["lakecatReplayVerification"]["storageProfileUpsertProof"]["secretRefHash"] =
        json!(qglake_fixture_hash("storage-secret-ref"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject credential secret-ref state drift");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("secretRefPresent"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_trusted_human_exception_reason() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["rawCredentialExceptionReason"] =
        json!("because I feel like it");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unaudited trusted-human exception reasons");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("rawCredentialExceptionReason"));
    assert!(
        err.to_string()
            .contains(QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON)
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_trusted_human_null_block_reason() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]
        .as_object_mut()
        .unwrap()
        .remove("blockReason");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should require trusted-human block reason proof");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("blockReason"));

    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["credentialVendingProof"]["trustedHuman"]["blockReason"] =
        json!("blocked");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject blocked trusted-human credential proof");

    assert!(err.to_string().contains("credentialVendingProof"));
    assert!(err.to_string().contains("blockReason must be null"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_view_tombstone_expected_version() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["expectedViewVersion"] =
        Value::Null;

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unguarded view tombstones");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("expectedViewVersion"));
    assert!(err.to_string().contains("non-negative integer"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_view_receipt_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptHash"] =
        json!("sha256:view-receipt");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short view receipt hashes");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("acceptedReceiptHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_tombstone_version_mismatch() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["expectedViewVersion"] =
        json!(99);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject stale view tombstone guards");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("expectedViewVersion mismatch"));
    assert!(err.to_string().contains("expected=1 actual=99"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_tombstone_stable_id_component_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["name"] =
        json!("other_view");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject tombstone identity component drift");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(
        err.to_string()
            .contains("stableId must match warehouse/namespace/name")
    );
}

#[test]
fn qglake_handoff_summary_verifier_requires_view_accepted_receipt_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptHash"] =
        json!("not-a-sha256-hash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject invalid accepted view receipt hashes");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("acceptedReceiptHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_bootstrap_view_receipt_hash_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewVersionReceiptHashes"] =
        json!([qglake_fixture_hash("other-view-receipt")]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject spliced bootstrap view receipt hashes");

    assert!(
        err.to_string()
            .contains("queryGraphBootstrapProof.viewVersionReceiptHashes")
    );
    assert!(err.to_string().contains("acceptedReceiptHash"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_short_bootstrap_view_receipt_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewVersionReceiptHashes"] =
        json!(["sha256:view-receipt"]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject short bootstrap view receipt hashes");

    assert!(
        err.to_string()
            .contains("queryGraphBootstrapProof.viewVersionReceiptHashes")
    );
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_view_graph_events() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["graphEvents"] =
        json!(0);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing accepted-view graph projection");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("graphEvents"));
    assert!(err.to_string().contains("positive"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_view_accepted_receipt_chain_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptChainHash"] =
        json!("not-a-sha256-hash");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject invalid accepted view receipt-chain hashes");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("acceptedReceiptChainHash"));
    assert!(err.to_string().contains("full SHA-256"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_uncovered_view_receipt_chain_hash() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptChainHash"] =
        json!(qglake_fixture_hash("uncovered-view-receipt-chain"));
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"] = json!([]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject accepted view chain hashes not covered by namespace chain evidence");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("acceptedReceiptChainHash"));
    assert!(err.to_string().contains("receiptChains"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_cross_view_receipt_chain_hash_splice() {
    let mut summary = qglake_handoff_summary_json();
    let (other_receipt, other_receipt_hash) =
        qglake_fixture_view_receipt("other_view", 1, None, None, "upsert");
    let other_chain_hash = qglake_fixture_view_chain_hash(
        "other_view",
        1,
        "upsert",
        false,
        std::slice::from_ref(&other_receipt_hash),
    );
    qglake_add_other_view_receipt_chain(
        &mut summary,
        other_chain_hash.clone(),
        other_receipt,
        other_receipt_hash,
    );
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptChainHash"] =
        json!(other_chain_hash);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject cross-view accepted chain hash splicing");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(
        err.to_string()
            .contains("same view's receiptChains[].chains[].chainHash")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_cross_view_tombstone_receipt_hash_splice() {
    let mut summary = qglake_handoff_summary_json();
    let (other_receipt, other_receipt_hash) =
        qglake_fixture_view_receipt("other_view", 1, None, None, "upsert");
    let other_chain_hash = qglake_fixture_view_chain_hash(
        "other_view",
        1,
        "upsert",
        false,
        std::slice::from_ref(&other_receipt_hash),
    );
    qglake_add_other_view_receipt_chain(
        &mut summary,
        other_chain_hash,
        other_receipt,
        other_receipt_hash.clone(),
    );
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["receiptHashes"] =
        json!([other_receipt_hash]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject cross-view tombstone receipt splicing");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(
        err.to_string()
            .contains("same view's receiptChains[].chains[].receipts[].receiptHash")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_tombstoned_uncovered_view_receipt_chain_hash() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptChainHash"] = json!(
        qglake_fixture_hash("uncovered-tombstoned-view-receipt-chain")
    );

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject tombstoned accepted views whose accepted chain hash is not covered by namespace chain evidence");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("acceptedReceiptChainHash"));
    assert!(err.to_string().contains("receiptChains"));
    assert!(
        err.to_string()
            .contains("lakecat:view:local:default:active_customers_view")
    );
    assert!(err.to_string().contains("version 1"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_invalid_view_receipt_chain_head() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["receipts"][0]["operation"] = json!("drop");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject invalid view receipt-chain heads");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("version 1 upsert"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_forged_view_receipt_previous_link() {
    let mut summary = qglake_handoff_summary_json();
    qglake_set_two_receipt_view_chain(&mut summary);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["receipts"][1]["previousReceiptHash"] = json!(qglake_fixture_hash("forged-previous"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject forged view receipt previous links");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("previous links"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_skipped_view_receipt_version() {
    let mut summary = qglake_handoff_summary_json();
    qglake_set_two_receipt_view_chain(&mut summary);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["receipts"][1]["viewVersion"] = json!(3);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject skipped view receipt versions");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("transition is invalid"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_unsupported_view_receipt_operation() {
    let mut summary = qglake_handoff_summary_json();
    qglake_set_two_receipt_view_chain(&mut summary);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["receipts"][1]["operation"] = json!("replace");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unsupported view receipt operations");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("transition is invalid"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_receipt_chain_group_identity_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["warehouse"] = json!("other");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject receipt-chain group identity drift");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(
        err.to_string()
            .contains("warehouse must match receipt-chain group warehouse")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_receipt_chain_receipt_identity_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["receipts"][0]["stableId"] = json!("lakecat:view:local:default:other_view");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject receipt-chain receipt identity drift");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(
        err.to_string()
            .contains("stableId must match chain stableId")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_stable_id_component_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["name"] =
        json!("other_view");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject accepted-view identity component drift");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(
        err.to_string()
            .contains("stableId must match warehouse/namespace/name")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_receipt_chain_stable_id_component_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["name"] = json!("other_view");
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["receipts"][0]["name"] = json!("other_view");

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject receipt-chain identity component drift");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(
        err.to_string()
            .contains("stableId must match warehouse/namespace/name")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_count_mismatch() {
    let mut summary = qglake_handoff_summary_json();
    summary["querygraphVerification"]["viewCount"] = json!(2);
    summary["querygraphVerification"]["verifiedViews"] = json!([
        "lakecat:view:local:default:active_customers_view",
        "lakecat:view:local:default:other_view"
    ]);
    summary["querygraphImportVerification"]["viewCount"] = json!(2);
    summary["querygraphImportVerification"]["verifiedViews"] = json!([
        "lakecat:view:local:default:active_customers_view",
        "lakecat:view:local:default:other_view"
    ]);
    summary["lakecatReplayVerification"]["queryGraphBootstrapProof"]["viewArtifactCount"] =
        json!(2);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["viewCount"] = json!(2);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject view-count drift");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("views length mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_unaccepted_view_version() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedViewVersion"] =
        json!(2);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["expectedViewVersion"] =
        json!(2);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unaccepted view version evidence");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("accepted view version"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_view_receipt_chain_identity() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["namespace"] =
        json!([]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject receipt chains without namespace identity");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("namespace"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_view_receipt_chain_hashes() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chainHashes"] =
        json!([]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject missing view receipt-chain hashes");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("chainHashes"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_view_receipt_chain_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_chain_hash = qglake_fixture_hash("view-receipt-chain");
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chainHashes"] =
        json!([duplicate_chain_hash.clone(), duplicate_chain_hash.clone()]);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [1]["chainHash"] = json!(duplicate_chain_hash);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate view receipt-chain hashes");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("chainHashes"));
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_structural_view_chain_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let first_chain =
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
            [0]
        .clone();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [1] = first_chain;

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate structural view chain hashes");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("chainHash"));
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_chain_hash_digest_drift() {
    let mut summary = qglake_handoff_summary_json();
    let forged_chain_hash = qglake_fixture_hash("forged-view-receipt-chain");
    let tombstone_chain_hash =
        summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
            [1]["chainHash"]
            .clone();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["acceptedReceiptChainHash"] =
        json!(forged_chain_hash.clone());
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chainHashes"] =
        json!([forged_chain_hash.clone(), tombstone_chain_hash]);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["chainHash"] = json!(forged_chain_hash);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject structural receipt-chain digest drift");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("chainHash"));
    assert!(err.to_string().contains("structural receipt-chain digest"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_receipt_hash_digest_drift() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["receipts"][0]["viewHash"] = json!(qglake_fixture_hash("forged-view-hash"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject structural view receipt digest drift");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("receiptHash"));
    assert!(err.to_string().contains("structural view receipt digest"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_duplicate_view_receipt_hashes() {
    let mut summary = qglake_handoff_summary_json();
    let duplicate_receipt_hash = qglake_fixture_hash("view-receipt");
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["receiptHashes"] =
        json!([duplicate_receipt_hash.clone(), duplicate_receipt_hash]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject duplicate view receipt hashes");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("receiptHashes"));
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn qglake_handoff_summary_verifier_requires_verified_view_receipt_chain_count() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["verifiedChainCount"] =
        json!(0);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject unverified view receipt chains");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("verifiedChainCount"));
    assert!(err.to_string().contains("positive"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_receipt_chain_count_mismatch() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["verifiedChainCount"] =
        json!(3);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject mismatched chain counts");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("verifiedChainCount mismatch"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_receipt_hash_undercoverage() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chainHashes"] =
        json!([
            qglake_fixture_hash("view-receipt-chain"),
            qglake_fixture_hash("chain-a"),
            qglake_fixture_hash("chain-b")
        ]);
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["verifiedChainCount"] =
        json!(3);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject receipt hashes that under-cover chains");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("receiptHashes"));
    assert!(err.to_string().contains("cover"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_view_receipt_hash_structural_mismatch() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["receiptHashes"] =
        json!([
            qglake_fixture_hash("view-receipt"),
            qglake_fixture_hash("extra-view-receipt")
        ]);

    let err = verify_qglake_handoff_summary_value(&summary).expect_err(
        "handoff summary should reject receipt hash arrays that do not match structural receipts",
    );

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("receiptHashes"));
    assert!(
        err.to_string()
            .contains("chains[].receipts[].receiptHash exactly")
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_uncovered_view_tombstone_receipts() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["receiptHashes"] =
        json!([qglake_fixture_hash("uncovered-tombstone")]);

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject tombstone receipts outside the namespace chain");

    assert!(err.to_string().contains("viewReceiptChainProof"));
    assert!(err.to_string().contains("tombstoneReceipts"));
    assert!(err.to_string().contains("receiptChains"));
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_view_receipt_chain_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["unverifiedViewClaim"] =
        json!(qglake_fixture_hash("unverified-view-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra view receipt-chain fields");
    let err = err.to_string();

    assert!(err.contains("viewReceiptChainProof"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedViewClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_accepted_view_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["views"][0]["unverifiedAcceptedViewClaim"] =
        json!(qglake_fixture_hash("unverified-accepted-view-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra accepted view fields");
    let err = err.to_string();

    assert!(err.contains("viewReceiptChainProof.views[]"), "{err}");
    assert!(
        err.contains("unexpected field unverifiedAcceptedViewClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_tombstone_receipt_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["tombstoneReceipts"][0]["unverifiedTombstoneClaim"] =
        json!(qglake_fixture_hash("unverified-tombstone-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra tombstone receipt fields");
    let err = err.to_string();

    assert!(
        err.contains("viewReceiptChainProof.tombstoneReceipts[]"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedTombstoneClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_receipt_chain_group_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["unverifiedChainGroupClaim"] =
        json!(qglake_fixture_hash("unverified-chain-group-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra receipt-chain group fields");
    let err = err.to_string();

    assert!(
        err.contains("viewReceiptChainProof.receiptChains[]"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedChainGroupClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_receipt_chain_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["unverifiedChainClaim"] = json!(qglake_fixture_hash("unverified-chain-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra receipt-chain fields");
    let err = err.to_string();

    assert!(
        err.contains("viewReceiptChainProof.receiptChains[].chains[]"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedChainClaim"),
        "{err}"
    );
}

#[test]
fn qglake_handoff_summary_verifier_rejects_extra_view_receipt_fields() {
    let mut summary = qglake_handoff_summary_json();
    summary["lakecatReplayVerification"]["viewReceiptChainProof"]["receiptChains"][0]["chains"]
        [0]["receipts"][0]["unverifiedReceiptClaim"] =
        json!(qglake_fixture_hash("unverified-receipt-claim"));

    let err = verify_qglake_handoff_summary_value(&summary)
        .expect_err("handoff summary should reject extra view receipt fields");
    let err = err.to_string();

    assert!(
        err.contains("viewReceiptChainProof.receiptChains[].chains[].receipts[]"),
        "{err}"
    );
    assert!(
        err.contains("unexpected field unverifiedReceiptClaim"),
        "{err}"
    );
}
