use std::sync::Arc;

use async_trait::async_trait;
use lakecat_api::StorageCredential;
#[cfg(not(feature = "sail-local"))]
use lakecat_core::sail::DeferredSailCatalogEngine;
use lakecat_core::sail::SailCatalogEngine;
use lakecat_core::{LakeCatError, Principal, WarehouseName};
use lakecat_graph::{CatalogGraphSink, NoopCatalogGraphSink};
use lakecat_lineage::{HashOnlyLineageSink, LineageSink};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_security::{AllowAllGovernanceEngine, AuthorizationReceipt, GovernanceEngine};
use lakecat_store::{CatalogStore, StorageProfile, TableRecord};
use serde::Deserialize;
use serde_json::Value;

use crate::*;

#[derive(Clone)]
pub struct LakeCatState {
    pub warehouse: WarehouseName,
    pub store: Arc<dyn CatalogStore>,
    pub sail: Arc<dyn SailCatalogEngine>,
    pub governance: Arc<dyn GovernanceEngine>,
    pub credential_issuer: Arc<dyn CredentialIssuer>,
    pub typedid_verifier: Arc<dyn TypeDidVerifier>,
    pub graph: Arc<dyn CatalogGraphSink>,
    pub lineage: Arc<dyn LineageSink>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ViewMutationQuery {
    #[serde(default)]
    pub(crate) expected_view_version: Option<u64>,
}

#[async_trait]
pub trait TypeDidVerifier: Send + Sync + 'static {
    async fn verify(&self, envelope_json: &str) -> Result<TypeDidVerification, LakeCatError>;
}

#[derive(Debug, Clone)]
pub struct TypeDidVerification {
    pub principal: Principal,
    pub attestation: Value,
}

#[derive(Debug, Default)]
pub struct ConservativeTypeDidVerifier;

impl ConservativeTypeDidVerifier {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl TypeDidVerifier for ConservativeTypeDidVerifier {
    async fn verify(&self, _envelope_json: &str) -> Result<TypeDidVerification, LakeCatError> {
        Err(LakeCatError::NotSupported(
            "TypeDID envelope verification requires the typesec-local integration".to_string(),
        ))
    }
}

#[async_trait]
pub trait CredentialIssuer: Send + Sync + 'static {
    async fn issue(
        &self,
        request: CredentialIssuanceRequest,
    ) -> Result<Vec<StorageCredential>, LakeCatError>;
}

#[derive(Debug, Clone)]
pub struct CredentialIssuanceRequest {
    pub table: TableRecord,
    pub profile: StorageProfile,
    pub authorization_receipt: AuthorizationReceipt,
    pub max_credential_ttl_seconds: Option<u64>,
}

#[derive(Debug, Default)]
pub struct ConservativeCredentialIssuer;

impl ConservativeCredentialIssuer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl CredentialIssuer for ConservativeCredentialIssuer {
    async fn issue(
        &self,
        request: CredentialIssuanceRequest,
    ) -> Result<Vec<StorageCredential>, LakeCatError> {
        if request.profile.can_return_public_credential() {
            return issued_credentials_for_profile(
                public_storage_credentials_for_profile(&request.profile),
                &request.profile,
                request.max_credential_ttl_seconds,
            );
        }
        Ok(Vec::new())
    }
}
impl LakeCatState {
    pub fn new(warehouse: WarehouseName, store: Arc<dyn CatalogStore>) -> Self {
        Self {
            warehouse,
            store,
            sail: default_sail_engine(),
            governance: AllowAllGovernanceEngine::new(),
            credential_issuer: ConservativeCredentialIssuer::new(),
            typedid_verifier: ConservativeTypeDidVerifier::new(),
            graph: NoopCatalogGraphSink::new(),
            lineage: HashOnlyLineageSink::new(),
        }
    }

    pub fn with_integrations(
        mut self,
        sail: Arc<dyn SailCatalogEngine>,
        governance: Arc<dyn GovernanceEngine>,
        graph: Arc<dyn CatalogGraphSink>,
        lineage: Arc<dyn LineageSink>,
    ) -> Self {
        self.sail = sail;
        self.governance = governance;
        self.graph = graph;
        self.lineage = lineage;
        self
    }

    pub fn with_credential_issuer(mut self, credential_issuer: Arc<dyn CredentialIssuer>) -> Self {
        self.credential_issuer = credential_issuer;
        self
    }

    pub fn with_typedid_verifier(mut self, typedid_verifier: Arc<dyn TypeDidVerifier>) -> Self {
        self.typedid_verifier = typedid_verifier;
        self
    }
}

#[cfg(feature = "sail-local")]
pub(crate) fn default_sail_engine() -> Arc<dyn SailCatalogEngine> {
    lakecat_sail::sail_integration::SailRestModelCatalogEngine::new()
}

#[cfg(not(feature = "sail-local"))]
pub(crate) fn default_sail_engine() -> Arc<dyn SailCatalogEngine> {
    DeferredSailCatalogEngine::new()
}
