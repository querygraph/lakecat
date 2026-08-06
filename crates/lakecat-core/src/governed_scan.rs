use serde::{Deserialize, Deserializer, Serialize};

use crate::{LakeCatError, LakeCatResult, TableIdent};

mod digest;
mod scope;
mod validation;

pub use digest::{
    governed_authorization_digest, governed_evidence_digest, governed_plan_digest,
    governed_policy_digest,
};
pub use scope::{
    GOVERNED_SCAN_SNAPSHOT_VERSION, GOVERNED_SCAN_SOURCE_SCOPE_VERSION,
    GovernedScanCatalogIdentity, GovernedScanDigests, governed_scan_digests,
};
pub use validation::{
    MAX_GOVERNED_SCAN_NAMESPACE_BYTES, MAX_GOVERNED_SCAN_NAMESPACE_COMPONENTS,
    MAX_GOVERNED_SCAN_PROJECTION_BYTES, MAX_GOVERNED_SCAN_PROJECTION_FIELDS,
    MAX_GOVERNED_SCAN_TEXT_BYTES, validate_governed_scan_projection,
    validate_governed_scan_requested_projection, validate_governed_scan_table,
    validate_governed_scan_text,
};

/// Wire version for the portable governed scan proof schema.
pub const GOVERNED_SCAN_PROOF_VERSION: &str = "lakecat.governed-scan-proof.v2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GovernedScanProofFields {
    version: String,
    grant_id: String,
    catalog_identity: GovernedScanCatalogIdentity,
    table: TableIdent,
    table_version: u64,
    snapshot_id: i64,
    plan_task_digest: String,
    principal_subject: String,
    purpose: String,
    effective_projection: Vec<String>,
    identity_context_digest: String,
    authorization_receipt_digest: String,
    policy_decision_digest: String,
}

/// Portable, secret-free proof binding a cognition input to a governed scan.
///
/// Fields are getter-only after issuance or decoding. Decoding accepts only
/// the current schema and immediately validates bounded structure and proof
/// integrity, so an unsupported or drifted value never becomes a typed proof.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct GovernedScanProof(GovernedScanProofFields);

/// Already-digested, stable evidence used to issue a governed scan proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedScanProofEvidence {
    pub catalog_identity: GovernedScanCatalogIdentity,
    pub table: TableIdent,
    pub table_version: u64,
    pub snapshot_id: i64,
    pub plan_task_digest: String,
    pub principal_subject: String,
    pub purpose: String,
    pub effective_projection: Vec<String>,
    pub identity_context_digest: String,
    pub authorization_receipt_digest: String,
    pub policy_decision_digest: String,
}

impl GovernedScanProof {
    /// Issue an integrity-bound proof from evidence that has already been
    /// reduced to stable, secret-free digests.
    pub fn issue(evidence: GovernedScanProofEvidence) -> LakeCatResult<Self> {
        validation::validate_evidence(&evidence)?;
        let grant_id = digest::proof_digest_from_evidence(&evidence)?;
        Ok(Self(GovernedScanProofFields {
            version: GOVERNED_SCAN_PROOF_VERSION.to_string(),
            grant_id,
            catalog_identity: evidence.catalog_identity,
            table: evidence.table,
            table_version: evidence.table_version,
            snapshot_id: evidence.snapshot_id,
            plan_task_digest: evidence.plan_task_digest,
            principal_subject: evidence.principal_subject,
            purpose: evidence.purpose,
            effective_projection: evidence.effective_projection,
            identity_context_digest: evidence.identity_context_digest,
            authorization_receipt_digest: evidence.authorization_receipt_digest,
            policy_decision_digest: evidence.policy_decision_digest,
        }))
    }

    /// Current governed-scan proof schema version.
    pub fn version(&self) -> &str {
        &self.0.version
    }

    /// Durable, integrity-bound grant identifier.
    pub fn grant_id(&self) -> &str {
        &self.0.grant_id
    }

    /// Service-owned catalog identity bound at issuance.
    pub fn catalog_identity(&self) -> &GovernedScanCatalogIdentity {
        &self.0.catalog_identity
    }

    /// Exact catalog table identity.
    pub fn table(&self) -> &TableIdent {
        &self.0.table
    }

    /// Catalog table version observed during planning.
    pub fn table_version(&self) -> u64 {
        self.0.table_version
    }

    /// Iceberg snapshot identifier observed during planning.
    pub fn snapshot_id(&self) -> i64 {
        self.0.snapshot_id
    }

    /// Digest of the Sail-produced scan tasks.
    pub fn plan_task_digest(&self) -> &str {
        &self.0.plan_task_digest
    }

    /// Principal subject authorized for the scan.
    pub fn principal_subject(&self) -> &str {
        &self.0.principal_subject
    }

    /// Purpose bound into the authorization decision.
    pub fn purpose(&self) -> &str {
        &self.0.purpose
    }

    /// Ordered, policy-narrowed projection.
    pub fn effective_projection(&self) -> &[String] {
        &self.0.effective_projection
    }

    /// Digest of the verified identity context.
    pub fn identity_context_digest(&self) -> &str {
        &self.0.identity_context_digest
    }

    /// Digest of the original authorization receipt.
    pub fn authorization_receipt_digest(&self) -> &str {
        &self.0.authorization_receipt_digest
    }

    /// Digest of the original policy decision.
    pub fn policy_decision_digest(&self) -> &str {
        &self.0.policy_decision_digest
    }

    /// Validate bounded, canonical proof fields without recomputing the proof
    /// identifier. This is safe to call at an untrusted serialization boundary.
    pub fn validate_structure(&self) -> LakeCatResult<()> {
        validation::validate_proof_structure(self)
    }

    /// Recompute the proof identifier and reject any field-level drift.
    pub fn validate_integrity(&self) -> LakeCatResult<()> {
        self.validate_structure()?;
        if digest::proof_digest_from_proof(self)? != self.grant_id() {
            return Err(LakeCatError::Conflict(
                "governed scan proof integrity validation failed".to_string(),
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for GovernedScanProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = GovernedScanProofFields::deserialize(deserializer)?;
        if fields.version != GOVERNED_SCAN_PROOF_VERSION {
            return Err(serde::de::Error::custom(
                "unsupported governed scan proof version",
            ));
        }
        let proof = Self(fields);
        proof
            .validate_integrity()
            .map_err(serde::de::Error::custom)?;
        Ok(proof)
    }
}

pub fn validate_governed_sha256_digest(digest: &str, label: &str) -> LakeCatResult<()> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Err(LakeCatError::InvalidArgument(format!(
            "governed scan {label} digest must be canonical lowercase SHA-256 evidence"
        )));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "governed scan {label} digest must be canonical lowercase SHA-256 evidence"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "governed_scan_tests.rs"]
mod tests;
