use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use super::digest::domain_hash;
use super::{GovernedScanProof, validate_governed_scan_text};
use crate::{LakeCatResult, TableIdent, WarehouseName, content_hash_bytes};

const SNAPSHOT_DOMAIN: &str = "lakecat.governed-scan-snapshot.digest.v1";
const SOURCE_SCOPE_DOMAIN: &str = "lakecat.governed-scan-source-scope.digest.v1";

/// Version embedded in canonical governed-scan snapshot evidence.
pub const GOVERNED_SCAN_SNAPSHOT_VERSION: &str = "lakecat.governed-scan-snapshot.v1";
/// Version embedded in canonical governed-scan source-scope evidence.
pub const GOVERNED_SCAN_SOURCE_SCOPE_VERSION: &str = "lakecat.governed-scan-source-scope.v1";

/// Stable catalog identity selected by trusted LakeCat service configuration.
///
/// This type validates canonical shape, not deployment authenticity. Remote
/// request data must never be promoted into a catalog identity; the service
/// owns one instance and binds it into every proof it issues.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct GovernedScanCatalogIdentity(String);

impl GovernedScanCatalogIdentity {
    /// Validate a catalog identity supplied by trusted process configuration.
    pub fn new(value: impl Into<String>) -> LakeCatResult<Self> {
        let value = value.into();
        validate_governed_scan_text(&value)?;
        Ok(Self(value))
    }

    /// Derive the default identity for a single-warehouse LakeCat service.
    pub fn for_warehouse(warehouse: &WarehouseName) -> Self {
        let readable = format!("lakecat://{}", warehouse.as_str());
        if validate_governed_scan_text(&readable).is_ok() {
            Self(readable)
        } else {
            let bounded = format!(
                "lakecat://warehouse/{}",
                content_hash_bytes(warehouse.as_str().as_bytes())
            );
            debug_assert!(validate_governed_scan_text(&bounded).is_ok());
            Self(bounded)
        }
    }

    /// Borrow the canonical identity text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for GovernedScanCatalogIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for GovernedScanCatalogIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// LakeCat-owned snapshot and grant-aware source identities from one proof
/// validation pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedScanDigests {
    snapshot_digest: String,
    source_scope_digest: String,
}

impl GovernedScanDigests {
    /// Catalog, table version, and snapshot identity without grant scope.
    pub fn snapshot_digest(&self) -> &str {
        &self.snapshot_digest
    }

    /// Snapshot identity composed with the durable governed grant.
    pub fn source_scope_digest(&self) -> &str {
        &self.source_scope_digest
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotDigestFields<'a> {
    version: &'static str,
    catalog_identity: &'a GovernedScanCatalogIdentity,
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

/// Compute the canonical snapshot and grant-aware source identities from one
/// integrity-validated, catalog-bound proof pass.
///
/// This pure canonicalizer does not reload the durable grant or revalidate
/// authorization. Authority consumers use the sealed result from
/// `lakecat-service`.
pub fn governed_scan_digests(proof: &GovernedScanProof) -> LakeCatResult<GovernedScanDigests> {
    validate_scope_inputs(proof)?;
    let snapshot_digest = snapshot_digest_after_validation(proof)?;
    let source_scope_digest = source_scope_digest(&snapshot_digest, proof.grant_id())?;
    Ok(GovernedScanDigests {
        snapshot_digest,
        source_scope_digest,
    })
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

fn validate_scope_inputs(proof: &GovernedScanProof) -> LakeCatResult<()> {
    proof.validate_integrity()
}

fn snapshot_digest_after_validation(proof: &GovernedScanProof) -> LakeCatResult<String> {
    domain_hash(
        SNAPSHOT_DOMAIN,
        &SnapshotDigestFields {
            version: GOVERNED_SCAN_SNAPSHOT_VERSION,
            catalog_identity: proof.catalog_identity(),
            table: proof.table(),
            table_version: proof.table_version(),
            snapshot_id: proof.snapshot_id(),
        },
    )
}
