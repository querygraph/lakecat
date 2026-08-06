use std::collections::BTreeSet;

use super::{
    GOVERNED_SCAN_PROOF_VERSION, GovernedScanProof, GovernedScanProofEvidence,
    validate_governed_sha256_digest,
};
use crate::{LakeCatError, LakeCatResult, TableIdent, validate_name};

/// Maximum UTF-8 bytes accepted for one governed-scan identity or field.
pub const MAX_GOVERNED_SCAN_TEXT_BYTES: usize = 4 * 1024;
/// Maximum fields accepted in one governed-scan projection.
pub const MAX_GOVERNED_SCAN_PROJECTION_FIELDS: usize = 256;
/// Maximum aggregate UTF-8 bytes accepted in one governed-scan projection.
pub const MAX_GOVERNED_SCAN_PROJECTION_BYTES: usize = 64 * 1024;
/// Maximum components accepted in one governed-scan namespace.
pub const MAX_GOVERNED_SCAN_NAMESPACE_COMPONENTS: usize = 256;
/// Maximum aggregate UTF-8 bytes accepted in one governed-scan namespace.
pub const MAX_GOVERNED_SCAN_NAMESPACE_BYTES: usize = 64 * 1024;

struct ProofFields<'a> {
    table: &'a TableIdent,
    snapshot_id: i64,
    plan_task_digest: &'a str,
    principal_subject: &'a str,
    purpose: &'a str,
    effective_projection: &'a [String],
    identity_context_digest: &'a str,
    authorization_receipt_digest: &'a str,
    policy_decision_digest: &'a str,
}

pub(super) fn validate_evidence(evidence: &GovernedScanProofEvidence) -> LakeCatResult<()> {
    validate_fields(ProofFields {
        table: &evidence.table,
        snapshot_id: evidence.snapshot_id,
        plan_task_digest: &evidence.plan_task_digest,
        principal_subject: &evidence.principal_subject,
        purpose: &evidence.purpose,
        effective_projection: &evidence.effective_projection,
        identity_context_digest: &evidence.identity_context_digest,
        authorization_receipt_digest: &evidence.authorization_receipt_digest,
        policy_decision_digest: &evidence.policy_decision_digest,
    })
}

pub(super) fn validate_proof_structure(proof: &GovernedScanProof) -> LakeCatResult<()> {
    if proof.version != GOVERNED_SCAN_PROOF_VERSION {
        return Err(invalid("unsupported governed scan proof version"));
    }
    validate_governed_sha256_digest(&proof.grant_id, "grant id")?;
    validate_fields(ProofFields {
        table: &proof.table,
        snapshot_id: proof.snapshot_id,
        plan_task_digest: &proof.plan_task_digest,
        principal_subject: &proof.principal_subject,
        purpose: &proof.purpose,
        effective_projection: &proof.effective_projection,
        identity_context_digest: &proof.identity_context_digest,
        authorization_receipt_digest: &proof.authorization_receipt_digest,
        policy_decision_digest: &proof.policy_decision_digest,
    })
}

fn validate_fields(fields: ProofFields<'_>) -> LakeCatResult<()> {
    if fields.snapshot_id < 0 {
        return Err(invalid(
            "governed scan proof snapshot identifier must not be negative",
        ));
    }
    validate_governed_scan_table(fields.table)?;
    validate_governed_scan_text(fields.principal_subject)?;
    validate_governed_scan_text(fields.purpose)?;
    validate_governed_scan_projection(fields.effective_projection)?;
    for (label, digest) in [
        ("plan task", fields.plan_task_digest),
        ("identity context", fields.identity_context_digest),
        ("authorization receipt", fields.authorization_receipt_digest),
        ("policy decision", fields.policy_decision_digest),
    ] {
        validate_governed_sha256_digest(digest, label)?;
    }
    Ok(())
}

/// Validate one bounded, whitespace-canonical governed-scan string.
pub fn validate_governed_scan_text(value: &str) -> LakeCatResult<()> {
    if value.is_empty()
        || value.len() > MAX_GOVERNED_SCAN_TEXT_BYTES
        || value != value.trim()
        || value.chars().any(char::is_control)
    {
        return Err(invalid(format!(
            "governed scan text must be canonical and at most \
             {MAX_GOVERNED_SCAN_TEXT_BYTES} UTF-8 bytes"
        )));
    }
    Ok(())
}

/// Re-run constructor validation for a possibly deserialized table identity.
pub fn validate_governed_scan_table(table: &TableIdent) -> LakeCatResult<()> {
    validate_governed_scan_text(table.warehouse.as_str())?;
    validate_governed_scan_text(table.name.as_str())?;
    validate_name("warehouse", table.warehouse.as_str())
        .map_err(|_| invalid("governed scan warehouse is not a valid catalog name"))?;
    validate_name("table", table.name.as_str())
        .map_err(|_| invalid("governed scan table is not a valid catalog name"))?;

    let parts = table.namespace.parts();
    if parts.is_empty() || parts.len() > MAX_GOVERNED_SCAN_NAMESPACE_COMPONENTS {
        return Err(invalid(format!(
            "governed scan namespace must contain at most \
             {MAX_GOVERNED_SCAN_NAMESPACE_COMPONENTS} components"
        )));
    }
    validate_string_budget(parts, "namespace", MAX_GOVERNED_SCAN_NAMESPACE_BYTES)?;
    for part in parts {
        validate_name("namespace component", part)
            .map_err(|_| invalid("governed scan namespace contains an invalid catalog name"))?;
    }
    Ok(())
}

/// Validate a required, ordered governed-scan projection.
pub fn validate_governed_scan_projection(fields: &[String]) -> LakeCatResult<()> {
    validate_projection(fields, false, "proof projection")
}

/// Validate an optional requested projection; empty means all allowed fields.
pub fn validate_governed_scan_requested_projection(fields: &[String]) -> LakeCatResult<()> {
    validate_projection(fields, true, "requested projection")
}

fn validate_projection(fields: &[String], allow_empty: bool, label: &str) -> LakeCatResult<()> {
    if fields.is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err(invalid("governed scan proof projection must not be empty"))
        };
    }
    if fields.len() > MAX_GOVERNED_SCAN_PROJECTION_FIELDS {
        return Err(invalid(format!(
            "governed scan {label} contains more than \
             {MAX_GOVERNED_SCAN_PROJECTION_FIELDS} fields"
        )));
    }
    validate_string_budget(fields, label, MAX_GOVERNED_SCAN_PROJECTION_BYTES)?;
    let unique = fields.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if unique.len() != fields.len() {
        return Err(invalid(format!(
            "governed scan {label} fields must be unique"
        )));
    }
    Ok(())
}

fn validate_string_budget(values: &[String], label: &str, maximum: usize) -> LakeCatResult<()> {
    let mut total = 0usize;
    for value in values {
        validate_governed_scan_text(value)?;
        total = total
            .checked_add(value.len())
            .ok_or_else(|| invalid(format!("governed scan {label} exceeds its byte limit")))?;
    }
    if total > maximum {
        return Err(invalid(format!(
            "governed scan {label} exceeds {maximum} aggregate UTF-8 bytes"
        )));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> LakeCatError {
    LakeCatError::InvalidArgument(message.into())
}
