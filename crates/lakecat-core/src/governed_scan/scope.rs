use serde::Serialize;

use super::digest::domain_hash;
use super::{GovernedScanProof, validate_governed_scan_text};
use crate::{LakeCatResult, TableIdent};

const SNAPSHOT_DOMAIN: &str = "lakecat.governed-scan-snapshot.digest.v1";
const SOURCE_SCOPE_DOMAIN: &str = "lakecat.governed-scan-source-scope.digest.v1";

/// Version embedded in canonical governed-scan snapshot evidence.
pub const GOVERNED_SCAN_SNAPSHOT_VERSION: &str = "lakecat.governed-scan-snapshot.v1";
/// Version embedded in canonical governed-scan source-scope evidence.
pub const GOVERNED_SCAN_SOURCE_SCOPE_VERSION: &str = "lakecat.governed-scan-source-scope.v1";

/// LakeCat-owned snapshot and grant-aware source identities from one proof
/// validation pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedScanDigests {
    /// Catalog, table version, and snapshot identity without grant scope.
    pub snapshot_digest: String,
    /// Snapshot identity composed with the durable governed grant.
    pub source_scope_digest: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotDigestFields<'a> {
    version: &'static str,
    catalog_identity: &'a str,
    table: &'a TableIdent,
    table_version: u64,
    snapshot_id: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceScopeFields<'a> {
    version: &'static str,
    snapshot_digest: &'a str,
    grant_id: &'a str,
}

/// Canonical identity of one catalog table version and snapshot. This does not
/// authenticate the caller that supplied `catalog_identity`.
pub fn governed_scan_snapshot_digest(
    catalog_identity: &str,
    proof: &GovernedScanProof,
) -> LakeCatResult<String> {
    validate_scope_inputs(catalog_identity, proof)?;
    snapshot_digest_after_validation(catalog_identity, proof)
}

/// Compute snapshot and grant-aware source identity after one validation pass.
pub fn governed_scan_digests(
    catalog_identity: &str,
    proof: &GovernedScanProof,
) -> LakeCatResult<GovernedScanDigests> {
    validate_scope_inputs(catalog_identity, proof)?;
    let snapshot_digest = snapshot_digest_after_validation(catalog_identity, proof)?;
    let source_scope_digest = source_scope_digest(&snapshot_digest, &proof.grant_id)?;
    Ok(GovernedScanDigests {
        snapshot_digest,
        source_scope_digest,
    })
}

/// Canonical identity of one validated snapshot plus its durable governed
/// grant. This does not authenticate the supplied catalog identity.
pub fn governed_scan_source_scope_digest(
    catalog_identity: &str,
    proof: &GovernedScanProof,
) -> LakeCatResult<String> {
    Ok(governed_scan_digests(catalog_identity, proof)?.source_scope_digest)
}

fn source_scope_digest(snapshot_digest: &str, grant_id: &str) -> LakeCatResult<String> {
    domain_hash(
        SOURCE_SCOPE_DOMAIN,
        &SourceScopeFields {
            version: GOVERNED_SCAN_SOURCE_SCOPE_VERSION,
            snapshot_digest,
            grant_id,
        },
    )
}

fn validate_scope_inputs(catalog_identity: &str, proof: &GovernedScanProof) -> LakeCatResult<()> {
    validate_governed_scan_text(catalog_identity)?;
    proof.validate_integrity()
}

fn snapshot_digest_after_validation(
    catalog_identity: &str,
    proof: &GovernedScanProof,
) -> LakeCatResult<String> {
    domain_hash(
        SNAPSHOT_DOMAIN,
        &SnapshotDigestFields {
            version: GOVERNED_SCAN_SNAPSHOT_VERSION,
            catalog_identity,
            table: &proof.table,
            table_version: proof.table_version,
            snapshot_id: proof.snapshot_id,
        },
    )
}
