use std::{net::SocketAddr, sync::Arc};

use lakecat_core::WarehouseName;
use lakecat_graph::CatalogGraphSink;
#[cfg(not(feature = "grust-local"))]
use lakecat_graph::NoopCatalogGraphSink;
use lakecat_lineage::HashOnlyLineageSink;
#[cfg(not(feature = "typesec-local"))]
use lakecat_security::AllowAllGovernanceEngine;
use lakecat_security::GovernanceEngine;
use lakecat_service::{CredentialIssuer, LakeCatState, app};
use lakecat_store::{CatalogStore, MemoryCatalogStore};

#[tokio::main]
async fn main() {
    let config = ServiceConfig::from_env().expect("configure LakeCat service");
    let state = LakeCatState::new(config.warehouse.clone(), configured_store().await);
    let state = {
        let sail = state.sail.clone();
        state
            .with_integrations(
                sail,
                configured_governance_engine().expect("configure LakeCat governance"),
                configured_graph_sink(),
                HashOnlyLineageSink::new(),
            )
            .with_credential_issuer(configured_credential_issuer())
    };
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .expect("bind LakeCat service");
    axum::serve(listener, app(state))
        .await
        .expect("serve LakeCat service");
}

struct ServiceConfig {
    warehouse: WarehouseName,
    bind_addr: SocketAddr,
}

impl ServiceConfig {
    fn from_env() -> lakecat_core::LakeCatResult<Self> {
        let warehouse = std::env::var("LAKECAT_WAREHOUSE").unwrap_or_else(|_| "local".to_string());
        let bind_addr = std::env::var("LAKECAT_BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8181".to_string())
            .parse::<SocketAddr>()
            .map_err(|err| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "invalid LAKECAT_BIND_ADDR: {err}"
                ))
            })?;
        Ok(Self {
            warehouse: WarehouseName::new(warehouse)?,
            bind_addr,
        })
    }
}

async fn configured_store() -> Arc<dyn CatalogStore> {
    configured_store_inner()
        .await
        .expect("configure LakeCat store")
}

#[cfg(feature = "turso-local")]
async fn configured_store_inner() -> lakecat_core::LakeCatResult<Arc<dyn CatalogStore>> {
    if let Ok(path) = std::env::var("LAKECAT_TURSO_PATH") {
        return Ok(lakecat_store::turso_store::TursoCatalogStore::connect_local(&path).await?);
    }
    Ok(MemoryCatalogStore::new())
}

#[cfg(not(feature = "turso-local"))]
async fn configured_store_inner() -> lakecat_core::LakeCatResult<Arc<dyn CatalogStore>> {
    Ok(MemoryCatalogStore::new())
}

#[cfg(feature = "typesec-local")]
fn configured_governance_engine() -> lakecat_core::LakeCatResult<Arc<dyn GovernanceEngine>> {
    configured_governance_engine_from_policy_path(
        std::env::var("LAKECAT_TYPESEC_RBAC_POLICY").ok().as_deref(),
    )
}

#[cfg(not(feature = "typesec-local"))]
fn configured_governance_engine() -> lakecat_core::LakeCatResult<Arc<dyn GovernanceEngine>> {
    Ok(AllowAllGovernanceEngine::new())
}

#[cfg(feature = "typesec-local")]
fn configured_governance_engine_from_policy_path(
    policy_path: Option<&str>,
) -> lakecat_core::LakeCatResult<Arc<dyn GovernanceEngine>> {
    let Some(policy_path) = policy_path else {
        return Ok(lakecat_security::typesec_integration::TypeSecGovernanceEngine::allow_all());
    };
    let yaml = std::fs::read_to_string(policy_path).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "failed to read LAKECAT_TYPESEC_RBAC_POLICY '{policy_path}': {err}"
        ))
    })?;
    Ok(lakecat_security::typesec_integration::TypeSecGovernanceEngine::rbac_from_yaml(&yaml)?)
}

#[cfg(feature = "typesec-local")]
fn configured_credential_issuer() -> Arc<dyn CredentialIssuer> {
    let issuer = lakecat_service::typesec_credential_issuer::TypeSecCredentialIssuer::allow_all_with_secret_ref_resolver();
    issuer
}

#[cfg(not(feature = "typesec-local"))]
fn configured_credential_issuer() -> Arc<dyn CredentialIssuer> {
    lakecat_service::ConservativeCredentialIssuer::new()
}

#[cfg(feature = "grust-local")]
fn configured_graph_sink() -> Arc<dyn CatalogGraphSink> {
    lakecat_graph::grust_integration::GrustCatalogGraphSink::new(Arc::new(
        grust_graph::MemoryGraphStore::new(),
    ))
}

#[cfg(not(feature = "grust-local"))]
fn configured_graph_sink() -> Arc<dyn CatalogGraphSink> {
    NoopCatalogGraphSink::new()
}

#[cfg(all(test, feature = "typesec-local"))]
mod tests {
    use super::*;
    use lakecat_core::{Namespace, Principal, PrincipalKind, TableIdent, TableName};
    use lakecat_security::{AuthorizationRequest, CatalogAction};

    #[tokio::test]
    async fn configured_governance_engine_loads_rbac_policy_path() {
        let path = std::env::temp_dir().join(format!(
            "lakecat-rbac-{}.yaml",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(
            &path,
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
        .unwrap();

        let engine = configured_governance_engine_from_policy_path(path.to_str()).unwrap();
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let receipt = engine
            .authorize(AuthorizationRequest {
                principal: Principal::new("agent:scanner", PrincipalKind::Agent).unwrap(),
                action: CatalogAction::TablePlanScan,
                table: Some(table),
                context: serde_json::json!({}),
            })
            .await
            .unwrap();

        assert!(receipt.allowed);
        assert_eq!(receipt.engine, "typesec");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn configured_governance_engine_rejects_missing_rbac_policy_path() {
        let missing = std::env::temp_dir().join(format!(
            "lakecat-missing-rbac-policy-{}.yaml",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let error = match configured_governance_engine_from_policy_path(missing.to_str()) {
            Ok(_) => panic!("missing policy path should fail"),
            Err(error) => error,
        };

        assert!(
            error
                .to_string()
                .contains("failed to read LAKECAT_TYPESEC_RBAC_POLICY")
        );
    }
}
