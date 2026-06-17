use std::sync::Arc;

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
    let state = LakeCatState::new(
        WarehouseName::new("local").expect("valid default warehouse"),
        configured_store().await,
    );
    let state = {
        let sail = state.sail.clone();
        state
            .with_integrations(
                sail,
                configured_governance_engine(),
                configured_graph_sink(),
                HashOnlyLineageSink::new(),
            )
            .with_credential_issuer(configured_credential_issuer())
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8181")
        .await
        .expect("bind LakeCat service");
    axum::serve(listener, app(state))
        .await
        .expect("serve LakeCat service");
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
fn configured_governance_engine() -> Arc<dyn GovernanceEngine> {
    lakecat_security::typesec_integration::TypeSecGovernanceEngine::allow_all()
}

#[cfg(not(feature = "typesec-local"))]
fn configured_governance_engine() -> Arc<dyn GovernanceEngine> {
    AllowAllGovernanceEngine::new()
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
