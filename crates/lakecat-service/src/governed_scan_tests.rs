use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use lakecat_core::sail::ScanPlan;
use lakecat_core::{Namespace, Principal, PrincipalKind, TableIdent, TableName, WarehouseName};
use lakecat_security::{AuthorizationReceipt, GovernanceEngine, TableScanCapability};
use lakecat_store::{CatalogStore, MemoryCatalogStore, PolicyBinding, TableCommit, TableRecord};
use serde_json::json;

use super::*;

#[derive(Debug)]
struct SwitchableGovernance {
    allowed: AtomicBool,
}

impl SwitchableGovernance {
    fn new(allowed: bool) -> Arc<Self> {
        Arc::new(Self {
            allowed: AtomicBool::new(allowed),
        })
    }
}

#[async_trait]
impl GovernanceEngine for SwitchableGovernance {
    async fn authorize(
        &self,
        request: AuthorizationRequest,
    ) -> LakeCatResult<AuthorizationReceipt> {
        Ok(AuthorizationReceipt {
            principal: request.principal,
            action: request.action,
            table: request.table,
            allowed: self.allowed.load(Ordering::SeqCst),
            engine: "test-governance".to_string(),
            policy_hash: None,
            context: request.context,
            checked_at: Utc::now(),
        })
    }
}

struct Fixture {
    state: LakeCatState,
    store: Arc<MemoryCatalogStore>,
    governance: Arc<SwitchableGovernance>,
    table: TableRecord,
    restriction: ReadRestriction,
}

impl Fixture {
    async fn new() -> Self {
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = Namespace::new(vec!["default".to_string()]).unwrap();
        let ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").unwrap(),
        );
        let store = MemoryCatalogStore::new();
        store
            .create_namespace(&warehouse, namespace.clone())
            .await
            .unwrap();
        let table = store
            .create_table(TableRecord::new(
                ident.clone(),
                "file:///events".to_string(),
                Some("file:///events/metadata/1.json".to_string()),
                json!({"format-version": 2, "current-snapshot-id": 42}),
                Principal::new("owner", PrincipalKind::Human).unwrap(),
            ))
            .await
            .unwrap();
        let binding = policy_binding(&ident, &["event_id"]);
        let restriction = ReadRestriction::from_odrl_policies([&binding.odrl]).unwrap();
        store.upsert_policy_binding(binding).await.unwrap();
        let governance = SwitchableGovernance::new(true);
        let state = LakeCatState::new(warehouse, store.clone());
        let state = LakeCatState {
            governance: governance.clone(),
            ..state
        };
        Self {
            state,
            store,
            governance,
            table,
            restriction,
        }
    }

    fn capability(&self, attestation_state: &str) -> TableScanCapability {
        let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
        let receipt = AuthorizationReceipt {
            principal: principal.clone(),
            action: CatalogAction::TablePlanScan,
            table: Some(self.table.ident.clone()),
            allowed: true,
            engine: "test-governance".to_string(),
            policy_hash: None,
            context: json!({
                "read-restriction": self.restriction,
                "request-identity": {
                    "principal": principal,
                    "attestation-state": attestation_state
                }
            }),
            checked_at: Utc::now(),
        }
        .with_read_restriction_policy_hash()
        .unwrap();
        TableScanCapability::from_receipt(receipt, self.table.ident.clone()).unwrap()
    }

    fn scan(&self) -> ScanPlan {
        ScanPlan {
            planned_by: "test".to_string(),
            snapshot_id: Some(42),
            scan_tasks: vec![json!({"plan-task": "opaque-secret"})],
            residual_filter: None,
        }
    }

    async fn grant(&self) -> GovernedScanProof {
        let grant = issue_governed_scan_grant(
            &self.capability("verified"),
            &self.table,
            &self.scan(),
            vec!["event_id".to_string()],
            vec!["event_id".to_string()],
        )
        .unwrap()
        .unwrap();
        self.store
            .save_governed_scan_grant(grant)
            .await
            .unwrap()
            .proof
    }
}

#[tokio::test]
async fn issuance_rejects_unverified_agent_identity() {
    let fixture = Fixture::new().await;
    let error = issue_governed_scan_grant(
        &fixture.capability("unverified"),
        &fixture.table,
        &fixture.scan(),
        vec!["event_id".to_string()],
        vec!["event_id".to_string()],
    )
    .unwrap_err();
    assert!(error.to_string().contains("verified TypeDID"));
}

fn policy_binding(table: &TableIdent, columns: &[&str]) -> PolicyBinding {
    PolicyBinding::new(
        "agent-read",
        table.warehouse.clone(),
        Some(table.namespace.clone()),
        Some(table.name.clone()),
        true,
        json!({
            "lakecat:read-restriction": {"allowed-columns": columns},
            "permission": [{
                "action": "read",
                "constraint": [{
                    "leftOperand": "purpose",
                    "operator": "eq",
                    "rightOperand": "marciana-cognition"
                }]
            }]
        }),
    )
    .unwrap()
}

#[tokio::test]
async fn revalidation_accepts_unchanged_governed_evidence() {
    let fixture = Fixture::new().await;
    let proof = fixture.grant().await;
    let revalidated = revalidate_governed_scan_grant(&fixture.state, &proof)
        .await
        .unwrap();
    assert_eq!(revalidated.grant.proof, proof);
    assert!(
        revalidated
            .fresh_authorization_digest
            .starts_with("sha256:")
    );
    assert_ne!(
        revalidated.fresh_authorization_digest,
        proof.authorization_receipt_digest
    );
    assert_eq!(
        revalidated.fresh_policy_decision_digest,
        proof.policy_decision_digest
    );
}

#[tokio::test]
async fn revalidation_rejects_stale_snapshot() {
    let fixture = Fixture::new().await;
    let proof = fixture.grant().await;
    fixture
        .store
        .commit_table(
            &fixture.table.ident,
            TableCommit {
                requirements: Vec::new(),
                updates: Vec::new(),
                expected_previous_metadata_location: fixture.table.metadata_location.clone(),
                new_metadata_location: Some("file:///events/metadata/2.json".to_string()),
                new_metadata: Some(json!({
                    "format-version": 2,
                    "current-snapshot-id": 43
                })),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::new("owner", PrincipalKind::Human).unwrap(),
                authorization_receipt: None,
            },
        )
        .await
        .unwrap();
    let error = revalidate_governed_scan_grant(&fixture.state, &proof)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("stale relative to current table")
    );
}

#[tokio::test]
async fn revalidation_rejects_revoked_authorization() {
    let fixture = Fixture::new().await;
    let proof = fixture.grant().await;
    fixture.governance.allowed.store(false, Ordering::SeqCst);
    let error = revalidate_governed_scan_grant(&fixture.state, &proof)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("authorization was revoked"));
}

#[tokio::test]
async fn revalidation_rejects_projection_change() {
    let fixture = Fixture::new().await;
    let proof = fixture.grant().await;
    fixture
        .store
        .upsert_policy_binding(policy_binding(&fixture.table.ident, &["payload"]))
        .await
        .unwrap();
    let error = revalidate_governed_scan_grant(&fixture.state, &proof)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("projection"));
}

#[tokio::test]
async fn revalidation_rejects_changed_presented_evidence() {
    let fixture = Fixture::new().await;
    let mut proof = fixture.grant().await;
    proof.plan_task_digest = governed_plan_digest(&[json!({"plan-task": "different"})]).unwrap();
    let error = revalidate_governed_scan_grant(&fixture.state, &proof)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("integrity"));
}
