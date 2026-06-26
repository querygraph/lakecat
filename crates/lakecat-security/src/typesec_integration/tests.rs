use std::sync::Arc;

use lakecat_core::{Principal, PrincipalKind};
use typesec::{PolicyEngine, PolicyResult, ResourceId, SubjectId};

use super::*;
use crate::{AuthorizationRequest, CatalogAction};

struct AllowRead;
struct DelegateToRbac;

impl PolicyEngine for AllowRead {
    fn check(&self, _subject: &SubjectId, action: &str, _resource: &ResourceId) -> PolicyResult {
        if action == "table.load" {
            PolicyResult::Allow
        } else {
            PolicyResult::Deny("not granted".to_string())
        }
    }
}

impl PolicyEngine for DelegateToRbac {
    fn check(&self, _subject: &SubjectId, _action: &str, _resource: &ResourceId) -> PolicyResult {
        PolicyResult::delegate("odrl", "rbac decides base access")
    }
}

#[tokio::test]
async fn delegates_authorization_to_typesec_policy_engine() {
    let engine = TypeSecGovernanceEngine::new(Arc::new(AllowRead));
    let receipt = engine
        .authorize(AuthorizationRequest {
            principal: Principal {
                subject: "agent:reader".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableLoad,
            table: None,
            context: serde_json::json!({}),
        })
        .await
        .expect("authorization should run");
    assert!(receipt.allowed);
    assert_eq!(receipt.engine, "typesec");
    assert!(receipt.policy_hash.is_some());
}

#[tokio::test]
async fn delegates_to_typesec_fallback_policy_engine() {
    let engine =
        TypeSecGovernanceEngine::with_fallback(Arc::new(DelegateToRbac), Arc::new(AllowRead));
    let receipt = engine
        .authorize(AuthorizationRequest {
            principal: Principal {
                subject: "agent:reader".to_string(),
                kind: PrincipalKind::Agent,
            },
            action: CatalogAction::TableLoad,
            table: None,
            context: serde_json::json!({"read-restriction": {"allowed-columns": ["id"]}}),
        })
        .await
        .expect("authorization should run through TypeSec fallback");
    assert!(receipt.allowed);
    assert_eq!(receipt.engine, "typesec");
    assert!(receipt.policy_hash.is_some());
    assert_eq!(
        receipt.context["read-restriction"]["allowed-columns"][0],
        serde_json::json!("id")
    );
}

#[tokio::test]
async fn loads_rbac_policy_yaml_for_authorization() {
    let engine = TypeSecGovernanceEngine::rbac_from_yaml(
        r#"
roles:
  - name: scanner
    permissions: ["table.plan_scan"]
    resources: ["lakecat:table:local:default:events"]
assignments:
  - subject: "agent:scanner"
    roles: [scanner]
"#,
    )
    .expect("rbac policy should load");
    let table = lakecat_core::TableIdent::new(
        lakecat_core::WarehouseName::new("local").unwrap(),
        "default".parse::<lakecat_core::Namespace>().unwrap(),
        lakecat_core::TableName::new("events").unwrap(),
    );
    let receipt = engine
        .authorize(AuthorizationRequest {
            principal: lakecat_core::Principal::new(
                "agent:scanner",
                lakecat_core::PrincipalKind::Agent,
            )
            .unwrap(),
            action: CatalogAction::TablePlanScan,
            table: Some(table),
            context: serde_json::json!({}),
        })
        .await
        .unwrap();

    assert!(receipt.allowed);
    assert_eq!(receipt.engine, "typesec");
}

#[test]
fn rejects_invalid_rbac_policy_yaml() {
    let error = match TypeSecGovernanceEngine::rbac_from_yaml(
        r#"
roles:
  - name: broken
    inherits: [missing]
"#,
    ) {
        Ok(_) => panic!("invalid rbac policy should fail closed"),
        Err(error) => error,
    };

    assert!(
        error
            .to_string()
            .contains("failed to load TypeSec RBAC policy")
    );
}
