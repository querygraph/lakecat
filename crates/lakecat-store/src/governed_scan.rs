use chrono::{DateTime, Utc};
use lakecat_core::governed_scan::{
    GovernedScanProof, validate_governed_scan_requested_projection, validate_governed_scan_text,
};
use lakecat_core::{LakeCatError, LakeCatResult, Principal};
use serde::{Deserialize, Serialize};

/// Durable, secret-free catalog evidence behind a governed scan proof.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GovernedScanGrant {
    pub proof: GovernedScanProof,
    pub principal: Principal,
    pub requested_projection: Vec<String>,
    pub policy_engine: String,
    pub policy_hash_digest: Option<String>,
    pub authorization_context_digest: String,
    pub read_restriction_digest: String,
    pub table_metadata_digest: String,
    pub issued_at: DateTime<Utc>,
}

impl GovernedScanGrant {
    pub fn validate(&self) -> LakeCatResult<()> {
        validate_governed_scan_text(&self.principal.subject)?;
        validate_governed_scan_text(&self.policy_engine)?;
        validate_governed_scan_requested_projection(&self.requested_projection)?;
        for (label, digest) in [
            (
                "authorization context",
                self.authorization_context_digest.as_str(),
            ),
            ("read restriction", self.read_restriction_digest.as_str()),
            ("table metadata", self.table_metadata_digest.as_str()),
        ] {
            validate_digest(digest, label)?;
        }
        if let Some(policy_hash) = self.policy_hash_digest.as_deref() {
            validate_digest(policy_hash, "policy")?;
        }
        self.proof.validate_integrity()?;
        if self.principal.subject != self.proof.principal_subject {
            return Err(LakeCatError::Conflict(
                "governed scan grant principal does not match its proof".to_string(),
            ));
        }
        Ok(())
    }

    pub(crate) fn has_same_stable_evidence(&self, other: &Self) -> bool {
        self.proof == other.proof
            && self.principal == other.principal
            && self.requested_projection == other.requested_projection
            && self.policy_engine == other.policy_engine
            && self.policy_hash_digest == other.policy_hash_digest
            && self.authorization_context_digest == other.authorization_context_digest
            && self.read_restriction_digest == other.read_restriction_digest
            && self.table_metadata_digest == other.table_metadata_digest
    }
}

pub(crate) fn validate_governed_scan_grant_id(value: &str) -> LakeCatResult<()> {
    validate_digest(value, "grant id")
}

fn validate_digest(value: &str, label: &str) -> LakeCatResult<()> {
    lakecat_core::governed_scan::validate_governed_sha256_digest(value, label)
}
