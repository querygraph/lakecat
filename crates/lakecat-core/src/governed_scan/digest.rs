use serde::Serialize;
use serde_json::{Value, json};

use super::{GOVERNED_SCAN_PROOF_VERSION, GovernedScanProof, GovernedScanProofEvidence};
use crate::{LakeCatError, LakeCatResult, TableIdent, content_hash_domain_json};

const PROOF_DOMAIN: &str = "lakecat.governed-scan-proof.digest.v2";
const PLAN_DOMAIN: &str = "lakecat.governed-scan-plan.digest.v1";
const AUTHORIZATION_DOMAIN: &str = "lakecat.authorization-decision.digest.v1";
const POLICY_DOMAIN: &str = "lakecat.scan-policy-decision.digest.v1";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProofDigestFields<'a> {
    version: &'static str,
    catalog_identity: &'a super::GovernedScanCatalogIdentity,
    table: &'a TableIdent,
    table_version: u64,
    snapshot_id: i64,
    plan_task_digest: &'a str,
    principal_subject: &'a str,
    purpose: &'a str,
    effective_projection: &'a [String],
    identity_context_digest: &'a str,
    authorization_receipt_digest: &'a str,
    policy_decision_digest: &'a str,
}

impl<'a> From<&'a GovernedScanProofEvidence> for ProofDigestFields<'a> {
    fn from(evidence: &'a GovernedScanProofEvidence) -> Self {
        Self {
            version: GOVERNED_SCAN_PROOF_VERSION,
            catalog_identity: &evidence.catalog_identity,
            table: &evidence.table,
            table_version: evidence.table_version,
            snapshot_id: evidence.snapshot_id,
            plan_task_digest: &evidence.plan_task_digest,
            principal_subject: &evidence.principal_subject,
            purpose: &evidence.purpose,
            effective_projection: &evidence.effective_projection,
            identity_context_digest: &evidence.identity_context_digest,
            authorization_receipt_digest: &evidence.authorization_receipt_digest,
            policy_decision_digest: &evidence.policy_decision_digest,
        }
    }
}

impl<'a> From<&'a GovernedScanProof> for ProofDigestFields<'a> {
    fn from(proof: &'a GovernedScanProof) -> Self {
        Self {
            version: GOVERNED_SCAN_PROOF_VERSION,
            catalog_identity: proof.catalog_identity(),
            table: proof.table(),
            table_version: proof.table_version(),
            snapshot_id: proof.snapshot_id(),
            plan_task_digest: proof.plan_task_digest(),
            principal_subject: proof.principal_subject(),
            purpose: proof.purpose(),
            effective_projection: proof.effective_projection(),
            identity_context_digest: proof.identity_context_digest(),
            authorization_receipt_digest: proof.authorization_receipt_digest(),
            policy_decision_digest: proof.policy_decision_digest(),
        }
    }
}

pub(super) fn proof_digest_from_evidence(
    evidence: &GovernedScanProofEvidence,
) -> LakeCatResult<String> {
    domain_hash(PROOF_DOMAIN, &ProofDigestFields::from(evidence))
}

pub(super) fn proof_digest_from_proof(proof: &GovernedScanProof) -> LakeCatResult<String> {
    domain_hash(PROOF_DOMAIN, &ProofDigestFields::from(proof))
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

pub(super) fn domain_hash(
    domain: &str,
    evidence: &(impl Serialize + ?Sized),
) -> LakeCatResult<String> {
    let mut evidence = serde_json::to_value(evidence).map_err(|error| {
        LakeCatError::Internal(format!("failed to encode governed scan evidence: {error}"))
    })?;
    canonicalize_json(&mut evidence);
    content_hash_domain_json(domain, &evidence)
}

fn canonicalize_json(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                canonicalize_json(value);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                canonicalize_json(value);
            }
            values.sort_keys();
        }
        _ => {}
    }
}
