use serde_json::json;

use super::evidence;
use crate::governed_scan::GovernedScanProof;

#[test]
fn legacy_v1_proof_fails_closed() {
    let proof = GovernedScanProof::issue(evidence()).unwrap();
    let mut encoded = serde_json::to_value(proof).unwrap();
    encoded["version"] = json!("lakecat.governed-scan-proof.v1");

    let error = serde_json::from_value::<GovernedScanProof>(encoded).unwrap_err();
    assert!(error.to_string().contains("unsupported"));
}

#[test]
fn proof_without_catalog_identity_fails_during_decode() {
    let proof = GovernedScanProof::issue(evidence()).unwrap();
    let mut encoded = serde_json::to_value(proof).unwrap();
    encoded.as_object_mut().unwrap().remove("catalogIdentity");

    assert!(serde_json::from_value::<GovernedScanProof>(encoded).is_err());
}

#[test]
fn unversioned_or_extended_proof_fails_during_decode() {
    let proof = GovernedScanProof::issue(evidence()).unwrap();

    let mut unversioned = serde_json::to_value(&proof).unwrap();
    unversioned.as_object_mut().unwrap().remove("version");
    assert!(serde_json::from_value::<GovernedScanProof>(unversioned).is_err());

    let mut extended = serde_json::to_value(proof).unwrap();
    extended["legacyGrant"] = json!(true);
    assert!(serde_json::from_value::<GovernedScanProof>(extended).is_err());
}
