use serde_json::json;

use super::evidence;
use crate::TableName;
use crate::governed_scan::{
    GOVERNED_SCAN_SNAPSHOT_VERSION, GOVERNED_SCAN_SOURCE_SCOPE_VERSION,
    GovernedScanCatalogIdentity, GovernedScanProof, MAX_GOVERNED_SCAN_TEXT_BYTES,
    governed_evidence_digest, governed_plan_digest, governed_scan_digests,
};

const SNAPSHOT_DOMAIN: &str = "lakecat.governed-scan-snapshot.digest.v1";
const SOURCE_SCOPE_DOMAIN: &str = "lakecat.governed-scan-source-scope.digest.v1";

#[test]
fn source_scope_uses_the_canonical_lakecat_shape_and_domain() {
    let proof = GovernedScanProof::issue(evidence()).unwrap();
    let snapshot_evidence = json!({
        "version": GOVERNED_SCAN_SNAPSHOT_VERSION,
        "catalogIdentity": "lakecat-production",
        "table": proof.table(),
        "tableVersion": proof.table_version(),
        "snapshotId": proof.snapshot_id(),
    });
    let expected_snapshot = governed_evidence_digest(SNAPSHOT_DOMAIN, &snapshot_evidence).unwrap();
    assert_eq!(
        governed_scan_digests(&proof).unwrap().snapshot_digest(),
        expected_snapshot
    );

    let scope_evidence = json!({
        "version": GOVERNED_SCAN_SOURCE_SCOPE_VERSION,
        "snapshotDigest": expected_snapshot,
        "grantId": proof.grant_id(),
    });
    let expected = governed_evidence_digest(SOURCE_SCOPE_DOMAIN, &scope_evidence).unwrap();
    let paired = governed_scan_digests(&proof).unwrap();
    assert_eq!(paired.snapshot_digest(), expected_snapshot);
    assert_eq!(paired.source_scope_digest(), expected);
    assert_ne!(
        paired.source_scope_digest(),
        governed_evidence_digest("lakecat.test.different-source-scope", &scope_evidence).unwrap()
    );
    assert_ne!(
        governed_scan_digests(&proof).unwrap().snapshot_digest(),
        governed_evidence_digest("lakecat.test.different-snapshot", &snapshot_evidence).unwrap()
    );
}

#[test]
fn source_scope_changes_for_every_owned_scope_dimension() {
    let base_evidence = evidence();
    let base = GovernedScanProof::issue(base_evidence.clone()).unwrap();
    let base_digest = governed_scan_digests(&base)
        .unwrap()
        .source_scope_digest()
        .to_string();

    let mut changed_catalog = base_evidence.clone();
    changed_catalog.catalog_identity = GovernedScanCatalogIdentity::new("lakecat-other").unwrap();
    assert_scope_changed(&base_digest, changed_catalog);

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
        governed_scan_digests(&base).unwrap().snapshot_digest(),
        governed_scan_digests(&changed_grant_proof)
            .unwrap()
            .snapshot_digest()
    );
    assert_scope_changed(&base_digest, changed_grant);
}

#[test]
fn snapshot_digest_changes_for_catalog_table_version_and_snapshot() {
    let base_evidence = evidence();
    let base = GovernedScanProof::issue(base_evidence.clone()).unwrap();
    let base_digest = governed_scan_digests(&base)
        .unwrap()
        .snapshot_digest()
        .to_string();

    let mut changed_catalog = base_evidence.clone();
    changed_catalog.catalog_identity = GovernedScanCatalogIdentity::new("lakecat-other").unwrap();
    assert_snapshot_changed(&base_digest, changed_catalog);

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
fn source_scope_rejects_uncanonical_or_drifted_proof_identity() {
    assert!(GovernedScanCatalogIdentity::new("c".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES)).is_ok());
    for catalog in ["", " lakecat", "lakecat\n"] {
        assert!(GovernedScanCatalogIdentity::new(catalog).is_err());
    }
    assert!(
        GovernedScanCatalogIdentity::new("c".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES + 1)).is_err()
    );

    let proof = GovernedScanProof::issue(evidence()).unwrap();
    let mut drifted = serde_json::to_value(proof).unwrap();
    drifted["catalogIdentity"] = json!("lakecat-other");
    assert!(serde_json::from_value::<GovernedScanProof>(drifted).is_err());
}

fn assert_scope_changed(
    base_digest: &str,
    evidence: crate::governed_scan::GovernedScanProofEvidence,
) {
    let proof = GovernedScanProof::issue(evidence).unwrap();
    assert_ne!(
        base_digest,
        governed_scan_digests(&proof).unwrap().source_scope_digest()
    );
}

fn assert_snapshot_changed(
    base_digest: &str,
    evidence: crate::governed_scan::GovernedScanProofEvidence,
) {
    let proof = GovernedScanProof::issue(evidence).unwrap();
    assert_ne!(
        base_digest,
        governed_scan_digests(&proof).unwrap().snapshot_digest()
    );
}
