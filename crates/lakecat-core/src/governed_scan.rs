use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{LakeCatError, LakeCatResult, TableIdent, content_hash_bytes};

/// Wire version for the portable governed scan proof schema.
pub const GOVERNED_SCAN_PROOF_VERSION: &str = "lakecat.governed-scan-proof.v1";
const PROOF_DOMAIN: &str = "lakecat.governed-scan-proof.digest.v1";
const PLAN_DOMAIN: &str = "lakecat.governed-scan-plan.digest.v1";
const AUTHORIZATION_DOMAIN: &str = "lakecat.authorization-decision.digest.v1";
const POLICY_DOMAIN: &str = "lakecat.scan-policy-decision.digest.v1";

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
        validate_evidence(&evidence)?;
        let grant_id = proof_digest(&evidence)?;
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

    /// Recompute the proof identifier and reject any field-level drift.
    pub fn validate_integrity(&self) -> LakeCatResult<()> {
        if self.version != GOVERNED_SCAN_PROOF_VERSION {
            return Err(LakeCatError::InvalidArgument(
                "unsupported governed scan proof version".to_string(),
            ));
        }
        validate_governed_sha256_digest(&self.grant_id, "grant id")?;
        let expected = Self::issue(GovernedScanProofEvidence {
            table: self.table.clone(),
            table_version: self.table_version,
            snapshot_id: self.snapshot_id,
            plan_task_digest: self.plan_task_digest.clone(),
            principal_subject: self.principal_subject.clone(),
            purpose: self.purpose.clone(),
            effective_projection: self.effective_projection.clone(),
            identity_context_digest: self.identity_context_digest.clone(),
            authorization_receipt_digest: self.authorization_receipt_digest.clone(),
            policy_decision_digest: self.policy_decision_digest.clone(),
        })?;
        if expected.grant_id != self.grant_id {
            return Err(LakeCatError::Conflict(
                "governed scan proof integrity validation failed".to_string(),
            ));
        }
        Ok(())
    }
}

pub fn governed_plan_digest(plan_tasks: &[Value]) -> LakeCatResult<String> {
    domain_hash(PLAN_DOMAIN, &json!({ "planTasks": plan_tasks }))
}

pub fn governed_authorization_digest(evidence: &Value) -> LakeCatResult<String> {
    domain_hash(AUTHORIZATION_DOMAIN, evidence)
}

pub fn governed_policy_digest(evidence: &Value) -> LakeCatResult<String> {
    domain_hash(POLICY_DOMAIN, evidence)
}

pub fn governed_evidence_digest(domain: &str, evidence: &Value) -> LakeCatResult<String> {
    if domain.trim().is_empty() || domain.contains('\0') {
        return Err(LakeCatError::InvalidArgument(
            "governed evidence digest domain must be non-blank and contain no NUL bytes"
                .to_string(),
        ));
    }
    domain_hash(domain, evidence)
}

fn proof_digest(evidence: &GovernedScanProofEvidence) -> LakeCatResult<String> {
    domain_hash(
        PROOF_DOMAIN,
        &json!({
            "version": GOVERNED_SCAN_PROOF_VERSION,
            "table": evidence.table,
            "tableVersion": evidence.table_version,
            "snapshotId": evidence.snapshot_id,
            "planTaskDigest": evidence.plan_task_digest,
            "principalSubject": evidence.principal_subject,
            "purpose": evidence.purpose,
            "effectiveProjection": evidence.effective_projection,
            "identityContextDigest": evidence.identity_context_digest,
            "authorizationReceiptDigest": evidence.authorization_receipt_digest,
            "policyDecisionDigest": evidence.policy_decision_digest,
        }),
    )
}

fn validate_evidence(evidence: &GovernedScanProofEvidence) -> LakeCatResult<()> {
    if evidence.snapshot_id < 0
        || evidence.principal_subject.trim().is_empty()
        || evidence.purpose.trim().is_empty()
        || evidence.effective_projection.is_empty()
    {
        return Err(LakeCatError::InvalidArgument(
            "governed scan proof requires a snapshot, subject, purpose, and projection".to_string(),
        ));
    }
    for (label, digest) in [
        ("plan task", evidence.plan_task_digest.as_str()),
        (
            "identity context",
            evidence.identity_context_digest.as_str(),
        ),
        (
            "authorization receipt",
            evidence.authorization_receipt_digest.as_str(),
        ),
        ("policy decision", evidence.policy_decision_digest.as_str()),
    ] {
        validate_governed_sha256_digest(digest, label)?;
    }
    if evidence
        .effective_projection
        .iter()
        .any(|column| column.trim().is_empty())
    {
        return Err(LakeCatError::InvalidArgument(
            "governed scan proof projection columns must not be blank".to_string(),
        ));
    }
    let unique_columns = evidence
        .effective_projection
        .iter()
        .collect::<std::collections::BTreeSet<_>>();
    if unique_columns.len() != evidence.effective_projection.len() {
        return Err(LakeCatError::InvalidArgument(
            "governed scan proof projection columns must be unique".to_string(),
        ));
    }
    Ok(())
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

fn domain_hash(domain: &str, evidence: &Value) -> LakeCatResult<String> {
    let canonical = canonical_json(evidence);
    let encoded = serde_json::to_vec(&canonical).map_err(|error| {
        LakeCatError::Internal(format!("failed to encode governed scan evidence: {error}"))
    })?;
    let mut input = Vec::with_capacity(domain.len() + encoded.len() + 2);
    input.extend_from_slice(domain.as_bytes());
    input.push(0);
    input.extend_from_slice(&encoded);
    Ok(content_hash_bytes(&input))
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        Value::Object(values) => {
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort();
            let mut canonical = Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonical_json(&values[key]));
            }
            Value::Object(canonical)
        }
        value => value.clone(),
    }
}

#[cfg(test)]
#[path = "governed_scan_tests.rs"]
mod tests;
