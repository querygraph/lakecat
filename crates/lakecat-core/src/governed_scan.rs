use serde::{Deserialize, Serialize};

use crate::{LakeCatError, LakeCatResult, TableIdent};

mod digest;
mod scope;
mod validation;

pub use digest::{
    governed_authorization_digest, governed_evidence_digest, governed_plan_digest,
    governed_policy_digest,
};
pub use scope::{
    GOVERNED_SCAN_SNAPSHOT_VERSION, GOVERNED_SCAN_SOURCE_SCOPE_VERSION, GovernedScanDigests,
    governed_scan_digests, governed_scan_snapshot_digest, governed_scan_source_scope_digest,
};
pub use validation::{
    MAX_GOVERNED_SCAN_NAMESPACE_BYTES, MAX_GOVERNED_SCAN_NAMESPACE_COMPONENTS,
    MAX_GOVERNED_SCAN_PROJECTION_BYTES, MAX_GOVERNED_SCAN_PROJECTION_FIELDS,
    MAX_GOVERNED_SCAN_TEXT_BYTES, validate_governed_scan_projection,
    validate_governed_scan_requested_projection, validate_governed_scan_table,
    validate_governed_scan_text,
};

/// Wire version for the portable governed scan proof schema.
pub const GOVERNED_SCAN_PROOF_VERSION: &str = "lakecat.governed-scan-proof.v1";

/// Portable, secret-free proof binding a cognition input to a governed scan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GovernedScanProof {
    pub version: String,
    pub grant_id: String,
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

/// Already-digested, stable evidence used to issue a governed scan proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedScanProofEvidence {
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
        Ok(Self {
            version: GOVERNED_SCAN_PROOF_VERSION.to_string(),
            grant_id,
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
        })
    }

    /// Validate bounded, canonical proof fields without recomputing the proof
    /// identifier. This is safe to call at an untrusted serialization boundary.
    pub fn validate_structure(&self) -> LakeCatResult<()> {
        validation::validate_proof_structure(self)
    }

    /// Recompute the proof identifier and reject any field-level drift.
    pub fn validate_integrity(&self) -> LakeCatResult<()> {
        self.validate_structure()?;
        if digest::proof_digest_from_proof(self)? != self.grant_id {
            return Err(LakeCatError::Conflict(
                "governed scan proof integrity validation failed".to_string(),
            ));
        }
        Ok(())
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
