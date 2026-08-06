use serde_json::json;

use super::evidence;
use crate::TableName;
use crate::governed_scan::{
    GOVERNED_SCAN_SNAPSHOT_VERSION, GOVERNED_SCAN_SOURCE_SCOPE_VERSION, GovernedScanProof,
    MAX_GOVERNED_SCAN_TEXT_BYTES, governed_evidence_digest, governed_plan_digest,
    governed_scan_digests, governed_scan_snapshot_digest, governed_scan_source_scope_digest,
};

const SNAPSHOT_DOMAIN: &str = "lakecat.governed-scan-snapshot.digest.v1";
const SOURCE_SCOPE_DOMAIN: &str = "lakecat.governed-scan-source-scope.digest.v1";

#[test]
fn source_scope_uses_the_canonical_lakecat_shape_and_domain() {
    let proof = GovernedScanProof::issue(evidence()).unwrap();
    let snapshot_evidence = json!({
        "version": GOVERNED_SCAN_SNAPSHOT_VERSION,
        "catalogIdentity": "lakecat-production",
        "table": proof.table,
        "tableVersion": proof.table_version,
        "snapshotId": proof.snapshot_id,
    });
    let expected_snapshot = governed_evidence_digest(SNAPSHOT_DOMAIN, &snapshot_evidence).unwrap();
    assert_eq!(
        governed_scan_snapshot_digest("lakecat-production", &proof).unwrap(),
        expected_snapshot
    );

    let scope_evidence = json!({
        "version": GOVERNED_SCAN_SOURCE_SCOPE_VERSION,
        "snapshotDigest": expected_snapshot,
        "grantId": proof.grant_id,
    });
    let expected = governed_evidence_digest(SOURCE_SCOPE_DOMAIN, &scope_evidence).unwrap();
    let paired = governed_scan_digests("lakecat-production", &proof).unwrap();
    assert_eq!(paired.snapshot_digest, expected_snapshot);
    assert_eq!(paired.source_scope_digest, expected);
    assert_eq!(
        governed_scan_source_scope_digest("lakecat-production", &proof).unwrap(),
        paired.source_scope_digest
    );
    assert_ne!(
        paired.source_scope_digest,
        governed_evidence_digest("lakecat.test.different-source-scope", &scope_evidence).unwrap()
    );
    assert_ne!(
        governed_scan_snapshot_digest("lakecat-production", &proof).unwrap(),
        governed_evidence_digest("lakecat.test.different-snapshot", &snapshot_evidence).unwrap()
    );
}

#[test]
fn source_scope_changes_for_every_owned_scope_dimension() {
    let base_evidence = evidence();
    let base = GovernedScanProof::issue(base_evidence.clone()).unwrap();
    let base_digest = governed_scan_source_scope_digest("lakecat-a", &base).unwrap();
    assert_ne!(
        base_digest,
        governed_scan_source_scope_digest("lakecat-b", &base).unwrap()
    );

    let mut changed_table = base_evidence.clone();
    changed_table.table.name = TableName::new("other_events").unwrap();
    assert_scope_changed(&base_digest, changed_table);

    let mut changed_version = base_evidence.clone();
    changed_version.table_version += 1;
    assert_scope_changed(&base_digest, changed_version);

    let mut changed_snapshot = base_evidence.clone();
    changed_snapshot.snapshot_id += 1;
    assert_scope_changed(&base_digest, changed_snapshot);

    let mut changed_grant = base_evidence;
    changed_grant.plan_task_digest = governed_plan_digest(&[json!({"task": "other"})]).unwrap();
    let changed_grant_proof = GovernedScanProof::issue(changed_grant.clone()).unwrap();
    assert_eq!(
        governed_scan_snapshot_digest("lakecat-a", &base).unwrap(),
        governed_scan_snapshot_digest("lakecat-a", &changed_grant_proof).unwrap()
    );
    assert_scope_changed(&base_digest, changed_grant);
}

#[test]
fn snapshot_digest_changes_for_catalog_table_version_and_snapshot() {
    let base_evidence = evidence();
    let base = GovernedScanProof::issue(base_evidence.clone()).unwrap();
    let base_digest = governed_scan_snapshot_digest("lakecat-a", &base).unwrap();
    assert_ne!(
        base_digest,
        governed_scan_snapshot_digest("lakecat-b", &base).unwrap()
    );

    let mut changed_table = base_evidence.clone();
    changed_table.table.name = TableName::new("other_events").unwrap();
    assert_snapshot_changed(&base_digest, changed_table);

    let mut changed_version = base_evidence.clone();
    changed_version.table_version += 1;
    assert_snapshot_changed(&base_digest, changed_version);

    let mut changed_snapshot = base_evidence;
    changed_snapshot.snapshot_id += 1;
    assert_snapshot_changed(&base_digest, changed_snapshot);
}

#[test]
fn source_scope_rejects_uncanonical_catalog_or_drifted_proof() {
    let proof = GovernedScanProof::issue(evidence()).unwrap();
    assert!(
        governed_scan_source_scope_digest(&"c".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES), &proof)
            .is_ok()
    );
    for catalog in ["", " lakecat", "lakecat\n"] {
        assert!(governed_scan_source_scope_digest(catalog, &proof).is_err());
    }
    assert!(
        governed_scan_source_scope_digest(&"c".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES + 1), &proof)
            .is_err()
    );

    let mut drifted = proof;
    drifted.snapshot_id += 1;
    assert!(governed_scan_snapshot_digest("lakecat", &drifted).is_err());
    assert!(governed_scan_source_scope_digest("lakecat", &drifted).is_err());
}

fn assert_scope_changed(
    base_digest: &str,
    evidence: crate::governed_scan::GovernedScanProofEvidence,
) {
    let proof = GovernedScanProof::issue(evidence).unwrap();
    assert_ne!(
        base_digest,
        governed_scan_source_scope_digest("lakecat-a", &proof).unwrap()
    );
}

fn assert_snapshot_changed(
    base_digest: &str,
    evidence: crate::governed_scan::GovernedScanProofEvidence,
) {
    let proof = GovernedScanProof::issue(evidence).unwrap();
    assert_ne!(
        base_digest,
        governed_scan_snapshot_digest("lakecat-a", &proof).unwrap()
    );
}
