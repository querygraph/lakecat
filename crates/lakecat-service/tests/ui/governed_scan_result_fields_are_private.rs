use chrono::{DateTime, Utc};
use lakecat_core::governed_scan::{GovernedScanDigests, GovernedScanProof};
use lakecat_service::governed_scan::RevalidatedGovernedScanGrant;

fn construct_digests(snapshot_digest: String, source_scope_digest: String) {
    let _ = GovernedScanDigests {
        snapshot_digest,
        source_scope_digest,
    };
}

fn construct_result(
    proof: GovernedScanProof,
    digests: GovernedScanDigests,
    fresh_authorization_digest: String,
    fresh_policy_decision_digest: String,
    revalidated_at: DateTime<Utc>,
) {
    let _ = RevalidatedGovernedScanGrant {
        proof,
        digests,
        fresh_authorization_digest,
        fresh_policy_decision_digest,
        revalidated_at,
    };
}

fn main() {}
