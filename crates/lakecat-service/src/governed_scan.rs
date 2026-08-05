use chrono::{DateTime, Utc};
use lakecat_core::governed_scan::{
    GovernedScanProof, GovernedScanProofEvidence, governed_authorization_digest,
    governed_evidence_digest, governed_plan_digest, governed_policy_digest,
};
use lakecat_core::sail::ScanPlan;
use lakecat_core::{LakeCatError, LakeCatResult};
use lakecat_security::{
    AuthorizationReceipt, AuthorizationRequest, CatalogAction, ReadRestriction, TableScanCapability,
};
use lakecat_store::{GovernedScanGrant, TableRecord};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{LakeCatState, policy_binding_response, read_restriction_from_policy_bindings};

const RESTRICTION_DOMAIN: &str = "lakecat.scan-read-restriction.digest.v1";
const AUTHORIZATION_CONTEXT_DOMAIN: &str = "lakecat.scan-authorization-context.digest.v1";
const TABLE_METADATA_DOMAIN: &str = "lakecat.table-metadata.digest.v1";
const IDENTITY_CONTEXT_DOMAIN: &str = "lakecat.verified-identity-context.digest.v1";
const POLICY_HASH_DOMAIN: &str = "lakecat.scan-policy-hash.digest.v1";

/// Durable grant evidence after a fresh catalog and policy revalidation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevalidatedGovernedScanGrant {
    pub grant: GovernedScanGrant,
    pub fresh_authorization_digest: String,
    pub fresh_policy_decision_digest: String,
    pub revalidated_at: DateTime<Utc>,
}

pub(crate) fn issue_governed_scan_grant(
    capability: &TableScanCapability,
    table: &TableRecord,
    scan: &ScanPlan,
    requested_projection: Vec<String>,
    effective_projection: Vec<String>,
) -> LakeCatResult<Option<GovernedScanGrant>> {
    let restriction = capability.read_restriction()?;
    let Some(purpose) = restriction.purpose.clone() else {
        return Ok(None);
    };
    let Some(snapshot_id) = scan.snapshot_id else {
        return Err(LakeCatError::Conflict(
            "governed scan planning did not bind a table snapshot".to_string(),
        ));
    };
    if table
        .metadata
        .get("current-snapshot-id")
        .and_then(Value::as_i64)
        != Some(snapshot_id)
    {
        return Err(LakeCatError::Conflict(
            "governed scan plan does not match the current table snapshot".to_string(),
        ));
    }
    let receipt = capability.receipt();
    let identity_context = verified_identity_context(receipt)?;
    let authorization_receipt_digest = authorization_digest(receipt)?;
    let policy_decision_digest = policy_digest(receipt, &restriction)?;
    let proof = GovernedScanProof::issue(GovernedScanProofEvidence {
        table: capability.table().clone(),
        table_version: table.version,
        snapshot_id,
        plan_task_digest: governed_plan_digest(&scan.scan_tasks)?,
        principal_subject: receipt.principal.subject.clone(),
        purpose,
        effective_projection,
        identity_context_digest: governed_evidence_digest(
            IDENTITY_CONTEXT_DOMAIN,
            identity_context,
        )?,
        authorization_receipt_digest,
        policy_decision_digest,
    })?;
    let grant = GovernedScanGrant {
        proof,
        principal: receipt.principal.clone(),
        requested_projection,
        policy_engine: receipt.engine.clone(),
        policy_hash_digest: receipt
            .policy_hash
            .as_ref()
            .map(|hash| governed_evidence_digest(POLICY_HASH_DOMAIN, &Value::String(hash.clone())))
            .transpose()?,
        authorization_context_digest: governed_evidence_digest(
            AUTHORIZATION_CONTEXT_DOMAIN,
            &receipt.context,
        )?,
        read_restriction_digest: restriction_digest(&restriction)?,
        table_metadata_digest: governed_evidence_digest(TABLE_METADATA_DOMAIN, &table.metadata)?,
        issued_at: Utc::now(),
    };
    grant.validate()?;
    Ok(Some(grant))
}

/// Reload a durable grant and freshly check catalog state and governance.
///
/// The original receipt digest remains in `grant.proof`; fresh decision
/// digests are returned separately so callers never substitute one binding for
/// the other.
pub async fn revalidate_governed_scan_grant(
    state: &LakeCatState,
    presented: &GovernedScanProof,
) -> LakeCatResult<RevalidatedGovernedScanGrant> {
    presented.validate_integrity()?;
    let grant = state
        .store
        .load_governed_scan_grant(&presented.grant_id)
        .await?;
    if &grant.proof != presented {
        return Err(LakeCatError::Conflict(
            "presented governed scan proof differs from durable grant evidence".to_string(),
        ));
    }
    let table = state.store.load_table(&presented.table).await?;
    validate_table_evidence(&grant, &table)?;

    let bindings = state
        .store
        .policy_bindings_for_table(&presented.table)
        .await?;
    let restriction = read_restriction_from_policy_bindings(&bindings)?;
    let receipt = fresh_authorization(state, &grant, &restriction, &bindings).await?;
    validate_fresh_restriction(&grant, &restriction)?;
    validate_fresh_policy(&grant, &receipt, &restriction)?;

    Ok(RevalidatedGovernedScanGrant {
        grant,
        fresh_authorization_digest: authorization_digest(&receipt)?,
        fresh_policy_decision_digest: policy_digest(&receipt, &restriction)?,
        revalidated_at: Utc::now(),
    })
}

fn validate_table_evidence(grant: &GovernedScanGrant, table: &TableRecord) -> LakeCatResult<()> {
    let current_snapshot = table
        .metadata
        .get("current-snapshot-id")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            LakeCatError::Conflict(
                "current table metadata does not contain a governed snapshot".to_string(),
            )
        })?;
    if current_snapshot != grant.proof.snapshot_id
        || table.version != grant.proof.table_version
        || governed_evidence_digest(TABLE_METADATA_DOMAIN, &table.metadata)?
            != grant.table_metadata_digest
    {
        return Err(LakeCatError::Conflict(
            "governed scan grant is stale relative to current table evidence".to_string(),
        ));
    }
    Ok(())
}

fn validate_fresh_restriction(
    grant: &GovernedScanGrant,
    restriction: &ReadRestriction,
) -> LakeCatResult<()> {
    if restriction.purpose.as_deref() != Some(grant.proof.purpose.as_str()) {
        return Err(LakeCatError::Conflict(
            "governed scan purpose is no longer authorized".to_string(),
        ));
    }
    let projection = restriction.effective_projection(&grant.requested_projection)?;
    if projection != grant.proof.effective_projection {
        return Err(LakeCatError::Conflict(
            "governed scan projection changed since planning".to_string(),
        ));
    }
    if restriction_digest(restriction)? != grant.read_restriction_digest {
        return Err(LakeCatError::Conflict(
            "governed scan read restriction changed since planning".to_string(),
        ));
    }
    Ok(())
}

async fn fresh_authorization(
    state: &LakeCatState,
    grant: &GovernedScanGrant,
    restriction: &ReadRestriction,
    bindings: &[lakecat_store::PolicyBinding],
) -> LakeCatResult<AuthorizationReceipt> {
    let context = json!({
        "warehouse": grant.proof.table.warehouse.as_str(),
        "request-identity": {
            "principal": grant.principal,
            "attestation-state": "governed-scan-revalidation",
            "original-authorization-context-digest": grant.authorization_context_digest,
        },
        "policy-bindings": bindings.iter().map(policy_binding_response).collect::<Vec<_>>(),
        "read-restriction": restriction,
        "governed-scan-grant-id": grant.proof.grant_id,
    });
    let receipt = state
        .governance
        .authorize(AuthorizationRequest {
            principal: grant.principal.clone(),
            action: CatalogAction::TablePlanScan,
            table: Some(grant.proof.table.clone()),
            context,
        })
        .await?;
    if !receipt.allowed {
        return Err(LakeCatError::Forbidden(
            "governed scan authorization was revoked".to_string(),
        ));
    }
    receipt.with_read_restriction_policy_hash()
}

fn validate_fresh_policy(
    grant: &GovernedScanGrant,
    receipt: &AuthorizationReceipt,
    restriction: &ReadRestriction,
) -> LakeCatResult<()> {
    if receipt.principal != grant.principal
        || receipt.action != CatalogAction::TablePlanScan
        || receipt.table.as_ref() != Some(&grant.proof.table)
    {
        return Err(LakeCatError::Conflict(
            "fresh governed scan authorization has different authority scope".to_string(),
        ));
    }
    let fresh_capability =
        TableScanCapability::from_receipt(receipt.clone(), grant.proof.table.clone())?;
    if fresh_capability.read_restriction()? != *restriction {
        return Err(LakeCatError::Conflict(
            "fresh governed scan authorization carries different read restrictions".to_string(),
        ));
    }
    if policy_digest(receipt, restriction)? != grant.proof.policy_decision_digest {
        return Err(LakeCatError::Conflict(
            "governed scan policy decision changed since planning".to_string(),
        ));
    }
    Ok(())
}

fn authorization_digest(receipt: &AuthorizationReceipt) -> LakeCatResult<String> {
    governed_authorization_digest(&json!({
        "principal": receipt.principal,
        "action": receipt.action,
        "table": receipt.table,
        "allowed": receipt.allowed,
        "engine": receipt.engine,
        "policyHash": receipt.policy_hash,
        "context": receipt.context,
        "checkedAt": receipt.checked_at,
    }))
}

fn policy_digest(
    receipt: &AuthorizationReceipt,
    restriction: &ReadRestriction,
) -> LakeCatResult<String> {
    governed_policy_digest(&json!({
        "principal": receipt.principal,
        "action": receipt.action,
        "table": receipt.table,
        "allowed": receipt.allowed,
        "engine": receipt.engine,
        "policyHash": receipt.policy_hash,
        "readRestriction": restriction,
    }))
}

fn restriction_digest(restriction: &ReadRestriction) -> LakeCatResult<String> {
    governed_evidence_digest(
        RESTRICTION_DOMAIN,
        &serde_json::to_value(restriction).map_err(|error| {
            LakeCatError::Internal(format!("failed to encode read restriction: {error}"))
        })?,
    )
}

fn verified_identity_context(receipt: &AuthorizationReceipt) -> LakeCatResult<&Value> {
    let identity = receipt.context.get("request-identity").ok_or_else(|| {
        LakeCatError::Conflict(
            "governed scan authorization is missing request identity evidence".to_string(),
        )
    })?;
    let identity_principal: lakecat_core::Principal =
        serde_json::from_value(identity.get("principal").cloned().ok_or_else(|| {
            LakeCatError::Conflict(
                "governed scan identity evidence is missing its principal".to_string(),
            )
        })?)
        .map_err(|error| {
            LakeCatError::Conflict(format!(
                "governed scan identity principal is malformed: {error}"
            ))
        })?;
    if identity_principal != receipt.principal {
        return Err(LakeCatError::Conflict(
            "governed scan identity principal does not match authorization".to_string(),
        ));
    }
    if receipt.principal.kind == lakecat_core::PrincipalKind::Agent
        && identity.get("attestation-state").and_then(Value::as_str) != Some("verified")
    {
        return Err(LakeCatError::Conflict(
            "governed agent scan requires verified TypeDID identity evidence".to_string(),
        ));
    }
    Ok(identity)
}

#[cfg(test)]
#[path = "governed_scan_tests.rs"]
mod tests;
