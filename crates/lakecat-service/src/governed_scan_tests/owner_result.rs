use lakecat_core::governed_scan::{GovernedScanCatalogIdentity, governed_scan_digests};

use super::*;

#[tokio::test]
async fn owner_result_returns_one_exact_digest_pair() {
    let fixture = Fixture::new().await;
    let proof = fixture.grant().await;
    let expected = governed_scan_digests(&proof).unwrap();

    let revalidated = revalidate_governed_scan_grant(&fixture.state, &proof)
        .await
        .unwrap();

    assert_eq!(revalidated.proof(), &proof);
    assert_eq!(
        revalidated.catalog_identity(),
        fixture.state.catalog_identity()
    );
    assert_eq!(revalidated.grant_id(), proof.grant_id());
    assert_eq!(revalidated.snapshot_digest(), expected.snapshot_digest());
    assert_eq!(
        revalidated.source_scope_digest(),
        expected.source_scope_digest()
    );
    assert_eq!(
        revalidated.effective_projection(),
        proof.effective_projection()
    );
}

#[tokio::test]
async fn configured_catalog_drift_fails_before_fresh_authorization() {
    let fixture = Fixture::new().await;
    let proof = fixture.grant().await;
    let other_state = fixture
        .state
        .clone()
        .with_catalog_identity(GovernedScanCatalogIdentity::new("lakecat://other").unwrap());

    let error = revalidate_governed_scan_grant(&other_state, &proof)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("different configured catalog"));
    assert_eq!(fixture.governance.authorization_count(), 0);
}
