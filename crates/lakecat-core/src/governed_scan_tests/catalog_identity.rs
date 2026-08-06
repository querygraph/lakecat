use serde_json::json;

use super::evidence;
use crate::WarehouseName;
use crate::governed_scan::{
    GovernedScanCatalogIdentity, GovernedScanProof, MAX_GOVERNED_SCAN_TEXT_BYTES,
};

const DEFAULT_PREFIX: &str = "lakecat://";

#[test]
fn warehouse_default_is_readable_when_it_fits_the_shared_bound() {
    let warehouse = WarehouseName::new("research").unwrap();
    let identity = GovernedScanCatalogIdentity::for_warehouse(&warehouse);

    assert_eq!(identity.as_str(), "lakecat://research");
    GovernedScanCatalogIdentity::new(identity.to_string()).unwrap();
}

#[test]
fn warehouse_default_cannot_bypass_the_catalog_identity_bound() {
    let exact_name = "w".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES - DEFAULT_PREFIX.len());
    let exact =
        GovernedScanCatalogIdentity::for_warehouse(&WarehouseName::new(exact_name).unwrap());
    assert_eq!(exact.as_str().len(), MAX_GOVERNED_SCAN_TEXT_BYTES);

    let oversized_name = "w".repeat(MAX_GOVERNED_SCAN_TEXT_BYTES);
    let bounded =
        GovernedScanCatalogIdentity::for_warehouse(&WarehouseName::new(oversized_name).unwrap());
    assert!(bounded.as_str().starts_with("lakecat://warehouse/sha256:"));
    assert!(bounded.as_str().len() <= MAX_GOVERNED_SCAN_TEXT_BYTES);
    GovernedScanCatalogIdentity::new(bounded.to_string()).unwrap();
}

#[test]
fn catalog_identity_tampering_invalidates_the_proof() {
    let proof = GovernedScanProof::issue(evidence()).unwrap();
    let mut encoded = serde_json::to_value(proof).unwrap();
    encoded["catalogIdentity"] = json!("lakecat-other");

    assert!(serde_json::from_value::<GovernedScanProof>(encoded).is_err());
}
