use chrono::Utc;
use lakecat_core::governed_scan::{
    GovernedScanProof, GovernedScanProofEvidence, governed_authorization_digest,
    governed_evidence_digest, governed_plan_digest, governed_policy_digest,
};
use lakecat_core::{Namespace, Principal, PrincipalKind, TableIdent, TableName, WarehouseName};
use serde_json::json;

use super::*;

fn sample_grant() -> GovernedScanGrant {
    let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
    let proof = GovernedScanProof::issue(GovernedScanProofEvidence {
        table: TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        ),
        table_version: 3,
        snapshot_id: 42,
        plan_task_digest: governed_plan_digest(&[json!({"plan-task": "secret-plan-token"})])
            .unwrap(),
        principal_subject: principal.subject.clone(),
        purpose: "marciana-cognition".to_string(),
        effective_projection: vec!["event_id".to_string()],
        identity_context_digest: governed_evidence_digest(
            "lakecat.test.identity",
            &json!({"subject": principal.subject.clone()}),
        )
        .unwrap(),
        authorization_receipt_digest: governed_authorization_digest(&json!({
            "receipt": "secret-authorization-receipt"
        }))
        .unwrap(),
        policy_decision_digest: governed_policy_digest(&json!({"policy": "allow"})).unwrap(),
    })
    .unwrap();
    GovernedScanGrant {
        proof,
        principal,
        requested_projection: vec!["event_id".to_string()],
        policy_engine: "typesec".to_string(),
        policy_hash_digest: Some(
            governed_evidence_digest("lakecat.test.policy", &json!({"policy": "allow"})).unwrap(),
        ),
        authorization_context_digest: governed_evidence_digest(
            "lakecat.test.context",
            &json!({"token": "secret-context"}),
        )
        .unwrap(),
        read_restriction_digest: governed_evidence_digest(
            "lakecat.test.restriction",
            &json!({"columns": ["event_id"]}),
        )
        .unwrap(),
        table_metadata_digest: governed_evidence_digest(
            "lakecat.test.metadata",
            &json!({"current-snapshot-id": 42}),
        )
        .unwrap(),
        issued_at: Utc::now(),
    }
}

#[tokio::test]
async fn memory_store_persists_idempotent_secret_free_grants() {
    let store = MemoryCatalogStore::new();
    let grant = sample_grant();
    store.save_governed_scan_grant(grant.clone()).await.unwrap();
    let mut repeated = grant.clone();
    repeated.issued_at += chrono::Duration::seconds(1);
    store.save_governed_scan_grant(repeated).await.unwrap();
    let loaded = store
        .load_governed_scan_grant(&grant.proof.grant_id)
        .await
        .unwrap();
    assert_eq!(loaded, grant);
    let encoded = serde_json::to_string(&loaded).unwrap();
    for secret in [
        "secret-plan-token",
        "secret-authorization-receipt",
        "secret-context",
    ] {
        assert!(!encoded.contains(secret));
    }

    let mut collision = grant;
    collision.policy_engine = "different-engine".to_string();
    let error = store.save_governed_scan_grant(collision).await.unwrap_err();
    assert!(error.to_string().contains("reused with different evidence"));
}

#[test]
fn grant_rejects_uppercase_store_evidence_digest() {
    let mut grant = sample_grant();
    grant.table_metadata_digest = format!(
        "sha256:{}",
        grant.table_metadata_digest[7..].to_ascii_uppercase()
    );
    let error = grant.validate().unwrap_err();
    assert!(error.to_string().contains("canonical lowercase"));
}

#[cfg(feature = "turso-local")]
#[tokio::test]
async fn turso_store_reopens_persisted_governed_scan_grants() {
    let path = temp_database_path();
    let grant = sample_grant();
    {
        let store = turso_store::TursoCatalogStore::connect_local(&path)
            .await
            .unwrap();
        store.save_governed_scan_grant(grant.clone()).await.unwrap();
    }
    let reopened = turso_store::TursoCatalogStore::connect_local(&path)
        .await
        .unwrap();
    let loaded = reopened
        .load_governed_scan_grant(&grant.proof.grant_id)
        .await
        .unwrap();
    assert_eq!(loaded, grant);
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "turso-local")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn turso_store_concurrently_recovers_idempotent_grant_saves() {
    let path = temp_database_path();
    let store = turso_store::TursoCatalogStore::connect_local(&path)
        .await
        .unwrap();
    let base = sample_grant();
    let mut handles = Vec::new();
    for offset in 0..4 {
        let store = store.clone();
        let mut grant = base.clone();
        grant.issued_at += chrono::Duration::seconds(offset);
        handles.push(tokio::spawn(async move {
            store.save_governed_scan_grant(grant).await
        }));
    }
    let mut saved = Vec::new();
    for handle in handles {
        saved.push(handle.await.unwrap().unwrap());
    }
    assert!(saved.iter().all(|grant| grant == &saved[0]));
    let loaded = store
        .load_governed_scan_grant(&base.proof.grant_id)
        .await
        .unwrap();
    assert_eq!(loaded, saved[0]);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[cfg(feature = "turso-local")]
fn temp_database_path() -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!(
            "lakecat-governed-scan-{}-{nonce}.db",
            std::process::id()
        ))
        .to_string_lossy()
        .into_owned()
}
