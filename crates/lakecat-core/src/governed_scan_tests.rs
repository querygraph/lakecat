use serde_json::json;

use super::*;
use crate::{Namespace, TableName, WarehouseName};

fn evidence() -> GovernedScanProofEvidence {
    GovernedScanProofEvidence {
        table: TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        table_version: 7,
        snapshot_id: 42,
        plan_task_digest: governed_plan_digest(&[json!({"plan-task": "secret-plan-token"})])
            .unwrap(),
        principal_subject: "did:key:agent".to_string(),
        purpose: "research".to_string(),
        effective_projection: vec!["finding".to_string()],
        identity_context_digest: governed_evidence_digest(
            "lakecat.test.identity",
            &json!({"subject": "did:key:agent"}),
        )
        .unwrap(),
        authorization_receipt_digest: governed_authorization_digest(&json!({
            "allowed": true,
            "receipt": "signed-receipt",
        }))
        .unwrap(),
        policy_decision_digest: governed_policy_digest(&json!({"policy": "current"})).unwrap(),
    }
}

#[test]
fn proof_is_secret_free_and_integrity_bound() {
    let proof = GovernedScanProof::issue(evidence()).unwrap();
    let encoded = serde_json::to_string(&proof).unwrap();
    assert!(!encoded.contains("secret-plan-token"));
    assert!(!encoded.contains("signed-receipt"));
    proof.validate_integrity().unwrap();

    let mut changed = proof;
    changed.snapshot_id += 1;
    assert!(changed.validate_integrity().is_err());
}

#[test]
fn evidence_hashing_is_canonical_and_domain_separated() {
    let left = json!({"b": 2, "a": {"d": 4, "c": 3}});
    let right = json!({"a": {"c": 3, "d": 4}, "b": 2});
    assert_eq!(
        governed_evidence_digest("lakecat.test.left", &left).unwrap(),
        governed_evidence_digest("lakecat.test.left", &right).unwrap()
    );
    assert_ne!(
        governed_evidence_digest("lakecat.test.left", &left).unwrap(),
        governed_evidence_digest("lakecat.test.right", &left).unwrap()
    );
}

#[test]
fn proof_rejects_noncanonical_digest_encodings() {
    let mut uppercase = evidence();
    uppercase.plan_task_digest = format!(
        "sha256:{}",
        uppercase.plan_task_digest[7..].to_ascii_uppercase()
    );
    let error = GovernedScanProof::issue(uppercase).unwrap_err();
    assert!(error.to_string().contains("canonical lowercase"));

    let mut truncated = evidence();
    truncated.plan_task_digest = "sha256:abcd".to_string();
    let error = GovernedScanProof::issue(truncated).unwrap_err();
    assert!(error.to_string().contains("canonical lowercase"));

    let mut proof = GovernedScanProof::issue(evidence()).unwrap();
    proof.grant_id = format!("sha256:{}", proof.grant_id[7..].to_ascii_uppercase());
    let error = proof.validate_integrity().unwrap_err();
    assert!(error.to_string().contains("canonical lowercase"));
}
