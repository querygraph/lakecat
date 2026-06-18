use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use lakecat_api::{
    CatalogConfigResponse, CommitTableRequest, CommitTableResponse, ConfigEntry,
    CreateNamespaceRequest, CreateTableRequest, FetchScanTasksRequest as ApiFetchScanTasksRequest,
    FetchScanTasksResponse, LineageDrainEventSummary, LineageDrainResponse, ListNamespacesResponse,
    ListPolicyBindingsResponse, ListProjectsResponse, ListServersResponse,
    ListStorageProfilesResponse, ListViewsResponse, ListWarehousesResponse,
    LoadCredentialsResponse, LoadTableResponse, NamespaceResponse, PlanTableScanRequest,
    PlanTableScanResponse, PolicyBindingResponse, ProjectResponse, ServerResponse,
    StorageCredential, StorageProfileResponse, TableIdentifier, UpsertPolicyBindingRequest,
    UpsertProjectRequest, UpsertServerRequest, UpsertStorageProfileRequest, UpsertViewRequest,
    UpsertWarehouseRequest, ViewColumnResponse, ViewResponse, WarehouseResponse,
};
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, PrincipalKind, TableIdent, TableName,
    WarehouseName, content_hash_bytes, content_hash_json,
};
use lakecat_graph::{CatalogGraphSink, GraphAction, GraphEvent, NoopCatalogGraphSink};
use lakecat_lineage::{
    HashOnlyLineageSink, LineageEvent, LineageEventType, LineageReceipt, LineageSink,
};
use lakecat_querygraph::QueryGraphBootstrap;
#[cfg(not(feature = "sail-local"))]
use lakecat_sail::DeferredSailCatalogEngine;
#[cfg(not(feature = "sail-local"))]
use lakecat_sail::FetchScanTasksRequest as SailFetchScanTasksRequest;
#[cfg(not(feature = "sail-local"))]
use lakecat_sail::ScanPlanningRequest;
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_sail::{CommitPreparationRequest, SailCatalogEngine};
use lakecat_security::{
    AllowAllGovernanceEngine, AuthorizationReceipt, AuthorizationRequest, CatalogAction,
    CatalogConfigCapability, CredentialsVendCapability, GovernanceEngine, GraphReadCapability,
    LineageReadCapability, NamespaceCreateCapability, NamespaceDropCapability,
    NamespaceListCapability, NamespaceLoadCapability, PolicyManageCapability,
    ProjectManageCapability, ReadRestriction, ServerManageCapability,
    StorageProfileManageCapability, TableCommitCapability, TableCreateCapability,
    TableDropCapability, TableLoadCapability, TableRestoreCapability, TableScanCapability,
    ViewDropCapability, ViewLoadCapability, ViewManageCapability, WarehouseManageCapability,
};
use lakecat_store::{
    CatalogAuditEvent, CatalogStore, CredentialIssuanceMode, OutboxEvent, PolicyBinding,
    ProjectRecord, ServerRecord, StorageProfile, StorageProvider, TableCommit, TableRecord,
    ViewColumnRecord, ViewRecord, WarehouseRecord, table_ident,
};
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};
use serde_json::{Value, json};
use url::Url;

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
            return Ok(public_storage_credentials_for_profile(&request.profile));
        }
        Ok(Vec::new())
    }
}

#[cfg(feature = "typesec-local")]
pub mod typesec_credential_issuer {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use lakecat_api::{ConfigEntry, StorageCredential};
    use lakecat_core::{LakeCatError, LakeCatResult};
    use lakecat_store::CredentialIssuanceMode;
    use serde_json::Value;
    use typesec::{PolicyEngine, PolicyResult, ResourceId, SubjectId};
    use url::Url;

    use crate::{CredentialIssuanceRequest, CredentialIssuer};

    #[async_trait]
    pub trait SecretRefCredentialResolver: Send + Sync + 'static {
        async fn resolve(
            &self,
            request: &CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>>;
    }

    pub struct TypeSecCredentialIssuer {
        engine: Arc<dyn PolicyEngine>,
        resolver: Arc<dyn SecretRefCredentialResolver>,
    }

    impl TypeSecCredentialIssuer {
        pub fn new(
            engine: Arc<dyn PolicyEngine>,
            resolver: Arc<dyn SecretRefCredentialResolver>,
        ) -> Arc<Self> {
            Arc::new(Self { engine, resolver })
        }

        pub fn allow_all_demo() -> Arc<Self> {
            Self::new(Arc::new(AllowAllPolicy), Arc::new(NoopSecretRefResolver))
        }

        pub fn allow_all_with_env_resolver() -> Arc<Self> {
            Self::allow_all_with_secret_ref_resolver()
        }

        pub fn allow_all_with_secret_ref_resolver() -> Arc<Self> {
            Self::new(
                Arc::new(AllowAllPolicy),
                ExternalSecretRefCredentialResolver::new(),
            )
        }
    }

    #[async_trait]
    impl CredentialIssuer for TypeSecCredentialIssuer {
        async fn issue(
            &self,
            request: CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>> {
            if request.profile.can_return_public_credential() {
                return Ok(crate::public_storage_credentials_for_profile(
                    &request.profile,
                ));
            }
            if request.profile.issuance_mode != CredentialIssuanceMode::ShortLivedSecretRef {
                return Ok(Vec::new());
            }
            let Some(secret_ref) = request.profile.secret_ref.as_deref() else {
                return Err(LakeCatError::InvalidArgument(
                    "short-lived credential issuance requires a secret reference".to_string(),
                ));
            };
            secret_ref_provider(secret_ref)?;
            let subject = SubjectId::from(request.authorization_receipt.principal.subject.clone());
            let resource = ResourceId::from(secret_ref.to_string());
            let decision = self.engine.check(&subject, "credentials.issue", &resource);
            match decision {
                PolicyResult::Allow => self.resolver.resolve(&request).await,
                PolicyResult::Deny(reason) => Err(LakeCatError::Conflict(format!(
                    "TypeSec denied credential issuance: {reason}"
                ))),
                PolicyResult::Delegate(reason) => Err(LakeCatError::Conflict(format!(
                    "TypeSec delegated credential issuance without resolver policy: {reason}"
                ))),
                _ => Err(LakeCatError::Conflict(
                    "TypeSec returned an unsupported credential issuance decision".to_string(),
                )),
            }
        }
    }

    struct AllowAllPolicy;

    impl PolicyEngine for AllowAllPolicy {
        fn check(
            &self,
            _subject: &SubjectId,
            _action: &str,
            _resource: &ResourceId,
        ) -> PolicyResult {
            PolicyResult::Allow
        }
    }

    struct NoopSecretRefResolver;

    #[async_trait]
    impl SecretRefCredentialResolver for NoopSecretRefResolver {
        async fn resolve(
            &self,
            _request: &CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>> {
            Ok(Vec::new())
        }
    }

    pub struct StaticSecretRefCredentialResolver {
        credentials: BTreeMap<String, Vec<ConfigEntry>>,
    }

    impl StaticSecretRefCredentialResolver {
        pub fn new(credentials: BTreeMap<String, Vec<ConfigEntry>>) -> Arc<Self> {
            Arc::new(Self { credentials })
        }
    }

    #[async_trait]
    impl SecretRefCredentialResolver for StaticSecretRefCredentialResolver {
        async fn resolve(
            &self,
            request: &CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>> {
            let Some(secret_ref) = request.profile.secret_ref.as_deref() else {
                return Ok(Vec::new());
            };
            let Some(config) = self.credentials.get(secret_ref) else {
                return Ok(Vec::new());
            };
            Ok(vec![StorageCredential {
                prefix: request.profile.location_prefix.clone(),
                config: config.clone(),
            }])
        }
    }

    pub struct ExternalSecretRefCredentialResolver {
        env: Arc<EnvironmentSecretRefCredentialResolver>,
        vault: Option<Arc<VaultSecretRefCredentialResolver>>,
    }

    impl ExternalSecretRefCredentialResolver {
        pub fn new() -> Arc<Self> {
            Arc::new(Self {
                env: EnvironmentSecretRefCredentialResolver::new(),
                vault: VaultSecretRefCredentialResolver::from_env(),
            })
        }

        #[cfg(test)]
        pub fn with_env_reader(
            reader: impl Fn(&str) -> Result<String, std::env::VarError> + Send + Sync + 'static,
        ) -> Arc<Self> {
            Arc::new(Self {
                env: EnvironmentSecretRefCredentialResolver::with_reader(reader),
                vault: None,
            })
        }

        #[cfg(test)]
        pub fn with_vault(vault: Arc<VaultSecretRefCredentialResolver>) -> Arc<Self> {
            Arc::new(Self {
                env: EnvironmentSecretRefCredentialResolver::new(),
                vault: Some(vault),
            })
        }
    }

    #[async_trait]
    impl SecretRefCredentialResolver for ExternalSecretRefCredentialResolver {
        async fn resolve(
            &self,
            request: &CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>> {
            let Some(secret_ref) = request.profile.secret_ref.as_deref() else {
                return Ok(Vec::new());
            };
            match secret_ref_provider(secret_ref)? {
                SecretRefProvider::TypeSecEnv => self.env.resolve(request).await,
                SecretRefProvider::Vault => {
                    let Some(vault) = &self.vault else {
                        return Err(provider_not_configured(
                            SecretRefProvider::Vault,
                            secret_ref,
                        ));
                    };
                    vault.resolve(request).await
                }
                provider => Err(provider_not_configured(provider, secret_ref)),
            }
        }
    }

    pub struct VaultSecretRefCredentialResolver {
        address: Url,
        token: String,
        namespace: Option<String>,
        client: Arc<dyn VaultSecretClient>,
    }

    impl VaultSecretRefCredentialResolver {
        pub fn from_env() -> Option<Arc<Self>> {
            let address = std::env::var("LAKECAT_VAULT_ADDR")
                .or_else(|_| std::env::var("VAULT_ADDR"))
                .ok()?;
            let token = std::env::var("LAKECAT_VAULT_TOKEN")
                .or_else(|_| std::env::var("VAULT_TOKEN"))
                .ok()?;
            let namespace = std::env::var("LAKECAT_VAULT_NAMESPACE")
                .or_else(|_| std::env::var("VAULT_NAMESPACE"))
                .ok();
            Self::new(
                address,
                token,
                namespace,
                Arc::new(ReqwestVaultSecretClient),
            )
            .ok()
        }

        pub fn new(
            address: impl AsRef<str>,
            token: impl Into<String>,
            namespace: Option<String>,
            client: Arc<dyn VaultSecretClient>,
        ) -> LakeCatResult<Arc<Self>> {
            let address = Url::parse(address.as_ref()).map_err(|err| {
                LakeCatError::InvalidArgument(format!("invalid Vault address: {err}"))
            })?;
            let token = token.into();
            if token.trim().is_empty() {
                return Err(LakeCatError::InvalidArgument(
                    "Vault token must not be empty".to_string(),
                ));
            }
            Ok(Arc::new(Self {
                address,
                token,
                namespace,
                client,
            }))
        }

        fn secret_url(&self, secret_ref: &str) -> LakeCatResult<String> {
            let path = vault_secret_path(secret_ref)?;
            let mut base = self.address.clone();
            base.set_path(&format!(
                "{}/{}",
                base.path().trim_end_matches('/'),
                path.trim_start_matches('/')
            ));
            base.set_query(None);
            base.set_fragment(None);
            Ok(base.to_string())
        }
    }

    #[async_trait]
    impl SecretRefCredentialResolver for VaultSecretRefCredentialResolver {
        async fn resolve(
            &self,
            request: &CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>> {
            let Some(secret_ref) = request.profile.secret_ref.as_deref() else {
                return Ok(Vec::new());
            };
            let url = self.secret_url(secret_ref)?;
            let secret = self
                .client
                .read_secret(&url, &self.token, self.namespace.as_deref())
                .await?;
            Ok(vec![StorageCredential {
                prefix: request.profile.location_prefix.clone(),
                config: config_entries_from_vault_secret_json(secret)?,
            }])
        }
    }

    #[async_trait]
    pub trait VaultSecretClient: Send + Sync + 'static {
        async fn read_secret(
            &self,
            url: &str,
            token: &str,
            namespace: Option<&str>,
        ) -> LakeCatResult<Value>;
    }

    struct ReqwestVaultSecretClient;

    #[async_trait]
    impl VaultSecretClient for ReqwestVaultSecretClient {
        async fn read_secret(
            &self,
            url: &str,
            token: &str,
            namespace: Option<&str>,
        ) -> LakeCatResult<Value> {
            let client = reqwest::Client::new();
            let mut request = client.get(url).header("X-Vault-Token", token);
            if let Some(namespace) = namespace {
                request = request.header("X-Vault-Namespace", namespace);
            }
            let response = request.send().await.map_err(|err| {
                LakeCatError::InvalidArgument(format!("failed to read Vault secret: {err}"))
            })?;
            let status = response.status();
            if !status.is_success() {
                return Err(LakeCatError::InvalidArgument(format!(
                    "Vault secret read failed with status {status}"
                )));
            }
            response.json::<Value>().await.map_err(|err| {
                LakeCatError::InvalidArgument(format!("Vault secret response must be JSON: {err}"))
            })
        }
    }

    pub struct EnvironmentSecretRefCredentialResolver {
        reader: Arc<dyn Fn(&str) -> Result<String, std::env::VarError> + Send + Sync>,
    }

    impl EnvironmentSecretRefCredentialResolver {
        pub fn new() -> Arc<Self> {
            Arc::new(Self {
                reader: Arc::new(|name: &str| std::env::var(name)),
            })
        }

        #[cfg(test)]
        pub fn with_reader(
            reader: impl Fn(&str) -> Result<String, std::env::VarError> + Send + Sync + 'static,
        ) -> Arc<Self> {
            Arc::new(Self {
                reader: Arc::new(reader),
            })
        }
    }

    #[async_trait]
    impl SecretRefCredentialResolver for EnvironmentSecretRefCredentialResolver {
        async fn resolve(
            &self,
            request: &CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>> {
            let Some(secret_ref) = request.profile.secret_ref.as_deref() else {
                return Ok(Vec::new());
            };
            let variable = env_secret_variable(secret_ref)?;
            let raw = (self.reader)(&variable).map_err(|err| {
                LakeCatError::InvalidArgument(format!(
                    "failed to resolve environment credential secret {variable}: {err}"
                ))
            })?;
            Ok(vec![StorageCredential {
                prefix: request.profile.location_prefix.clone(),
                config: config_entries_from_secret_json(&raw)?,
            }])
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum SecretRefProvider {
        TypeSecEnv,
        TypeSec,
        Vault,
        AwsSecretsManager,
        GcpSecretManager,
        AzureKeyVault,
    }

    impl SecretRefProvider {
        pub(crate) fn as_str(self) -> &'static str {
            match self {
                Self::TypeSecEnv => "typesec-env",
                Self::TypeSec => "typesec",
                Self::Vault => "vault",
                Self::AwsSecretsManager => "aws-secrets-manager",
                Self::GcpSecretManager => "gcp-secret-manager",
                Self::AzureKeyVault => "azure-key-vault",
            }
        }
    }

    pub(crate) fn secret_ref_provider(secret_ref: &str) -> LakeCatResult<SecretRefProvider> {
        let url = Url::parse(secret_ref).map_err(|err| {
            LakeCatError::InvalidArgument(format!("invalid credential secret ref URI: {err}"))
        })?;
        match url.scheme() {
            "typesec" if url.host_str() == Some("env") => Ok(SecretRefProvider::TypeSecEnv),
            "typesec" => Ok(SecretRefProvider::TypeSec),
            "vault" => Ok(SecretRefProvider::Vault),
            "aws-sm" => Ok(SecretRefProvider::AwsSecretsManager),
            "gcp-sm" => Ok(SecretRefProvider::GcpSecretManager),
            "azure-kv" => Ok(SecretRefProvider::AzureKeyVault),
            scheme => Err(LakeCatError::InvalidArgument(format!(
                "unsupported credential secret-ref scheme for TypeSec-gated issuance: {scheme}"
            ))),
        }
    }

    fn provider_not_configured(provider: SecretRefProvider, secret_ref: &str) -> LakeCatError {
        LakeCatError::InvalidArgument(format!(
            "credential secret resolver for {} is not configured; keep governed reads on Sail \
             or configure a production secret-store backend for {secret_ref}",
            provider.as_str()
        ))
    }

    pub(crate) fn vault_secret_path(secret_ref: &str) -> LakeCatResult<String> {
        let url = Url::parse(secret_ref)
            .map_err(|err| LakeCatError::InvalidArgument(format!("invalid Vault URI: {err}")))?;
        if url.scheme() != "vault" {
            return Err(LakeCatError::InvalidArgument(format!(
                "Vault resolver requires vault:// secret refs, got {secret_ref}"
            )));
        }
        let Some(mount) = url.host_str() else {
            return Err(LakeCatError::InvalidArgument(format!(
                "Vault secret ref must include a mount name: {secret_ref}"
            )));
        };
        let path = url.path().trim_start_matches('/');
        if path.is_empty() {
            return Err(LakeCatError::InvalidArgument(format!(
                "Vault secret ref must include a secret path: {secret_ref}"
            )));
        }
        Ok(format!("v1/{mount}/{path}"))
    }

    pub(crate) fn config_entries_from_vault_secret_json(
        value: Value,
    ) -> LakeCatResult<Vec<ConfigEntry>> {
        let payload = value
            .get("data")
            .and_then(|data| data.get("data").or(Some(data)))
            .ok_or_else(|| {
                LakeCatError::InvalidArgument(
                    "Vault secret response must contain a data object".to_string(),
                )
            })?;
        let Some(object) = payload.as_object() else {
            return Err(LakeCatError::InvalidArgument(
                "Vault secret data must be a JSON object".to_string(),
            ));
        };
        object
            .iter()
            .map(|(key, value)| {
                let Some(value) = value.as_str() else {
                    return Err(LakeCatError::InvalidArgument(format!(
                        "Vault credential config value for {key} must be a string"
                    )));
                };
                Ok(ConfigEntry::new(key.clone(), value.to_string()))
            })
            .collect()
    }

    pub(crate) fn env_secret_variable(secret_ref: &str) -> LakeCatResult<String> {
        let url = Url::parse(secret_ref).map_err(|err| {
            LakeCatError::InvalidArgument(format!("invalid TypeSec secret ref URI: {err}"))
        })?;
        if url.scheme() != "typesec" || url.host_str() != Some("env") {
            return Err(LakeCatError::InvalidArgument(format!(
                "environment resolver requires secret refs like typesec://env/VARIABLE, got {secret_ref}"
            )));
        }
        let variable = url.path().trim_start_matches('/');
        if variable.is_empty()
            || !variable
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err(LakeCatError::InvalidArgument(format!(
                "environment credential variable must be non-empty and use A-Z, 0-9, or _: {variable}"
            )));
        }
        Ok(variable.to_string())
    }

    pub(crate) fn config_entries_from_secret_json(raw: &str) -> LakeCatResult<Vec<ConfigEntry>> {
        let value: Value = serde_json::from_str(raw).map_err(|err| {
            LakeCatError::InvalidArgument(format!(
                "environment credential secret must be JSON: {err}"
            ))
        })?;
        match value {
            Value::Object(object) => object
                .into_iter()
                .map(|(key, value)| {
                    let Some(value) = value.as_str() else {
                        return Err(LakeCatError::InvalidArgument(format!(
                            "credential config value for {key} must be a string"
                        )));
                    };
                    Ok(ConfigEntry::new(key, value))
                })
                .collect(),
            Value::Array(entries) => entries
                .into_iter()
                .map(|entry| {
                    serde_json::from_value(entry).map_err(|err| {
                        LakeCatError::InvalidArgument(format!(
                            "credential config entries must match ConfigEntry JSON shape: {err}"
                        ))
                    })
                })
                .collect(),
            _ => Err(LakeCatError::InvalidArgument(
                "environment credential secret must be a JSON object or ConfigEntry array"
                    .to_string(),
            )),
        }
    }
}

#[cfg(feature = "typesec-local")]
pub mod typesec_typedid {
    use std::sync::Arc;

    use async_trait::async_trait;
    use lakecat_core::{LakeCatError, LakeCatResult, Principal, PrincipalKind};
    use typesec::{DidEnvelope, TypeDidGateway};

    use crate::{TypeDidVerification, TypeDidVerifier};

    pub struct TypeSecTypeDidVerifier {
        gateway: Arc<TypeDidGateway>,
    }

    impl TypeSecTypeDidVerifier {
        pub fn new(gateway: Arc<TypeDidGateway>) -> Arc<Self> {
            Arc::new(Self { gateway })
        }
    }

    #[async_trait]
    impl TypeDidVerifier for TypeSecTypeDidVerifier {
        async fn verify(&self, envelope_json: &str) -> LakeCatResult<TypeDidVerification> {
            let envelope: DidEnvelope = serde_json::from_str(envelope_json).map_err(|err| {
                LakeCatError::InvalidArgument(format!("invalid TypeDID envelope JSON: {err}"))
            })?;
            let verified = self.gateway.open_message(&envelope).map_err(|err| {
                LakeCatError::Conflict(format!("TypeSec rejected TypeDID envelope: {err}"))
            })?;
            let attestation = verified.attestation();
            Ok(TypeDidVerification {
                principal: Principal::new(attestation.subject.to_string(), PrincipalKind::Agent)?,
                attestation: serde_json::to_value(attestation).map_err(|err| {
                    LakeCatError::Internal(format!("failed to encode TypeDID attestation: {err}"))
                })?,
            })
        }
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
fn default_sail_engine() -> Arc<dyn SailCatalogEngine> {
    lakecat_sail::sail_integration::SailRestModelCatalogEngine::new()
}

#[cfg(not(feature = "sail-local"))]
fn default_sail_engine() -> Arc<dyn SailCatalogEngine> {
    DeferredSailCatalogEngine::new()
}

pub fn app(state: LakeCatState) -> Router {
    Router::new()
        .route("/catalog/v1/config", get(get_config))
        .route(
            "/catalog/v1/{warehouse}/config",
            get(get_config_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces",
            get(list_namespaces_for_warehouse).post(create_namespace_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}",
            get(load_namespace_for_warehouse).delete(drop_namespace_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables",
            post(create_table_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}",
            get(load_table_for_warehouse).delete(delete_table_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/commit",
            post(commit_table_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/plan",
            post(plan_table_scan_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
            post(fetch_scan_tasks_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/tasks",
            post(fetch_scan_tasks_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/credentials",
            get(load_credentials_for_warehouse),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/views",
            get(catalog_list_views),
        )
        .route(
            "/catalog/v1/{warehouse}/namespaces/{namespace}/views/{view}",
            get(catalog_load_view)
                .post(catalog_upsert_view)
                .put(catalog_upsert_view)
                .delete(catalog_drop_view),
        )
        .route(
            "/catalog/v1/namespaces",
            get(list_namespaces).post(create_namespace),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}",
            get(load_namespace).delete(drop_namespace),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables",
            post(create_table),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}",
            get(load_table).delete(delete_table),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/commit",
            post(commit_table),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/plan",
            post(plan_table_scan),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
            post(fetch_scan_tasks),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/tasks",
            post(fetch_scan_tasks),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
            get(load_credentials),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/tables/{table}/restore",
            post(restore_table),
        )
        .route("/management/v1/projects", get(list_projects))
        .route(
            "/management/v1/projects/{project}",
            post(upsert_project).put(upsert_project),
        )
        .route(
            "/management/v1/projects/{project}/warehouses",
            get(list_project_warehouses),
        )
        .route(
            "/management/v1/projects/{project}/warehouses/{warehouse}",
            post(upsert_project_warehouse).put(upsert_project_warehouse),
        )
        .route("/management/v1/warehouses", get(list_warehouses))
        .route(
            "/management/v1/warehouses/{warehouse}",
            post(upsert_warehouse).put(upsert_warehouse),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/storage-profiles",
            get(list_storage_profiles),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/storage-profiles/{profile}",
            post(upsert_storage_profile).put(upsert_storage_profile),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/views",
            get(list_views),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/views/{view}",
            post(upsert_view).put(upsert_view).delete(drop_view),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/policies",
            get(list_policy_bindings),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/policies/{policy}",
            post(upsert_policy_binding).put(upsert_policy_binding),
        )
        .route("/management/v1/lineage/drain", post(drain_lineage_outbox))
        .route("/management/v1/servers", get(list_servers))
        .route(
            "/management/v1/servers/{server}",
            post(upsert_server).put(upsert_server),
        )
        .route("/querygraph/v1/bootstrap", get(querygraph_bootstrap))
        .with_state(state)
}

#[derive(Debug, Default)]
struct OutboxProjectionReceipt {
    graph_events: usize,
    lineage_events: usize,
    lineage_event_hashes: Vec<String>,
    open_lineage_hashes: Vec<String>,
}

impl OutboxProjectionReceipt {
    fn record_lineage(&mut self, receipt: LineageReceipt) {
        self.lineage_events += 1;
        self.lineage_event_hashes.push(receipt.event_hash);
        self.open_lineage_hashes.push(receipt.open_lineage_hash);
    }
}

pub async fn drain_outbox_once(
    state: &LakeCatState,
    limit: usize,
) -> Result<LineageDrainResponse, LakeCatError> {
    let events = state
        .store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), limit)
        .await?;
    let mut delivered = Vec::with_capacity(events.len());
    let mut event_types = Vec::with_capacity(events.len());
    let mut summaries = Vec::with_capacity(events.len());
    let mut graph_events = 0usize;
    let mut lineage_events = 0usize;
    for event in events {
        let receipt = project_outbox_event(state, &event).await?;
        graph_events += receipt.graph_events;
        lineage_events += receipt.lineage_events;
        summaries.push(lineage_drain_event_summary(&event, &receipt));
        event_types.push(event.event_type.clone());
        delivered.push(event.event_id.clone());
    }
    let delivered = state.store.mark_outbox_delivered(&delivered).await?;
    Ok(LineageDrainResponse {
        delivered,
        event_types,
        graph_events,
        lineage_events,
        events: summaries,
    })
}

fn lineage_drain_event_summary(
    event: &OutboxEvent,
    receipt: &OutboxProjectionReceipt,
) -> LineageDrainEventSummary {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    LineageDrainEventSummary {
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        principal_subject: payload
            .pointer("/authorization-receipt/principal/subject")
            .and_then(Value::as_str)
            .map(str::to_string),
        principal_kind: payload
            .pointer("/authorization-receipt/principal/kind")
            .and_then(Value::as_str)
            .map(str::to_string),
        authorization_receipt_hash: payload
            .get("authorization-receipt")
            .and_then(|receipt| content_hash_json(receipt).ok()),
        request_identity_state: payload
            .pointer("/authorization-receipt/request-identity/attestation-state")
            .and_then(Value::as_str)
            .map(str::to_string),
        agent_delegation_hash: payload
            .pointer("/authorization-receipt/request-identity/agent-delegation-sha256")
            .and_then(Value::as_str)
            .map(str::to_string),
        agent_summary_signature_hash: payload
            .pointer("/authorization-receipt/request-identity/agent-summary-signature-sha256")
            .and_then(Value::as_str)
            .map(str::to_string),
        graph_events: receipt.graph_events,
        lineage_events: receipt.lineage_events,
        bundle_hash: payload
            .get("bundle-hash")
            .and_then(Value::as_str)
            .map(str::to_string),
        graph_hash: payload
            .get("graph-hash")
            .and_then(Value::as_str)
            .map(str::to_string),
        open_lineage_hash: payload
            .get("open-lineage-hash")
            .and_then(Value::as_str)
            .map(str::to_string),
        querygraph_import_hash: payload
            .get("querygraph-import-hash")
            .and_then(Value::as_str)
            .map(str::to_string),
        table_artifact_count: payload
            .get("table-artifacts")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        view_artifact_count: payload
            .get("view-artifacts")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        policy_binding_count: payload
            .get("policy-binding-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok())
            .unwrap_or_default(),
        standards: payload
            .get("standards")
            .and_then(Value::as_array)
            .map(|standards| {
                standards
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        credential_count: payload
            .get("credential-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        credential_block_reason: payload
            .get("lakecat:credential-block-reason")
            .and_then(Value::as_str)
            .map(str::to_string),
        raw_credential_exception_allowed: payload
            .pointer("/lakecat:raw-credential-exception/allowed")
            .and_then(Value::as_bool),
        raw_credential_exception_reason: payload
            .pointer("/lakecat:raw-credential-exception/reason")
            .and_then(Value::as_str)
            .map(str::to_string),
        replay_event_hashes: receipt.lineage_event_hashes.clone(),
        replay_open_lineage_hashes: receipt.open_lineage_hashes.clone(),
    }
}

async fn get_config(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<CatalogConfigResponse>, LakeCatHttpError> {
    get_config_in_warehouse(state.warehouse.clone(), state, headers).await
}

async fn get_config_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
) -> Result<Json<CatalogConfigResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    get_config_in_warehouse(warehouse, state, headers).await
}

async fn get_config_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
) -> Result<Json<CatalogConfigResponse>, LakeCatHttpError> {
    let capability = authorize_catalog_config(&state, request_identity(&headers)?).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "catalog.config-read",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "catalog.config-read",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
            }),
        )?)
        .await?;
    Ok(Json(CatalogConfigResponse::default()))
}

async fn create_namespace(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Json(request): Json<CreateNamespaceRequest>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    create_namespace_in_warehouse(state.warehouse.clone(), state, headers, request).await
}

async fn create_namespace_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
    Json(request): Json<CreateNamespaceRequest>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    create_namespace_in_warehouse(warehouse, state, headers, request).await
}

async fn create_namespace_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    request: CreateNamespaceRequest,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let capability = authorize_namespace_create(&state, identity).await?;
    let namespace = Namespace::new(request.namespace)?;
    state
        .store
        .create_namespace(&warehouse, namespace.clone())
        .await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "namespace.created",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "namespace.created",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
            }),
        )?)
        .await?;
    Ok(Json(NamespaceResponse::from_namespace(&namespace)))
}

async fn list_namespaces(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListNamespacesResponse>, LakeCatHttpError> {
    list_namespaces_in_warehouse(state.warehouse.clone(), state, headers).await
}

async fn list_namespaces_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
) -> Result<Json<ListNamespacesResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    list_namespaces_in_warehouse(warehouse, state, headers).await
}

async fn list_namespaces_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
) -> Result<Json<ListNamespacesResponse>, LakeCatHttpError> {
    let capability = authorize_namespace_list(&state, request_identity(&headers)?).await?;
    let namespaces = state.store.list_namespaces(&warehouse).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "namespace.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "namespace.listed",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
                "namespace-count": namespaces.len(),
            }),
        )?)
        .await?;
    Ok(Json(ListNamespacesResponse {
        namespaces: namespaces
            .into_iter()
            .map(|namespace| namespace.parts().to_vec())
            .collect(),
    }))
}

async fn load_namespace(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(namespace): Path<String>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    load_namespace_in_warehouse(state.warehouse.clone(), state, headers, namespace).await
}

async fn load_namespace_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    load_namespace_in_warehouse(warehouse, state, headers, namespace).await
}

async fn load_namespace_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let capability = authorize_namespace_load(&state, request_identity(&headers)?).await?;
    let namespace = namespace.parse::<Namespace>()?;
    let namespace = state.store.load_namespace(&warehouse, &namespace).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "namespace.loaded",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "namespace.loaded",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
            }),
        )?)
        .await?;
    Ok(Json(NamespaceResponse::from_namespace(&namespace)))
}

async fn drop_namespace(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(namespace): Path<String>,
) -> Result<StatusCode, LakeCatHttpError> {
    drop_namespace_in_warehouse(state.warehouse.clone(), state, headers, namespace).await
}

async fn drop_namespace_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    drop_namespace_in_warehouse(warehouse, state, headers, namespace).await
}

async fn drop_namespace_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
) -> Result<StatusCode, LakeCatHttpError> {
    let capability = authorize_namespace_drop(&state, request_identity(&headers)?).await?;
    let namespace = namespace.parse::<Namespace>()?;
    let namespace = state.store.drop_namespace(&warehouse, &namespace).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "namespace.dropped",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "namespace.dropped",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
            }),
        )?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(namespace): Path<String>,
    Json(request): Json<CreateTableRequest>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    create_table_in_warehouse(state.warehouse.clone(), state, headers, namespace, request).await
}

async fn create_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
    Json(request): Json<CreateTableRequest>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    create_table_in_warehouse(warehouse, state, headers, namespace, request).await
}

async fn create_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    request: CreateTableRequest,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(
        warehouse.as_str(),
        namespace,
        TableName::new(request.name)?.as_str(),
    )?;
    let capability = authorize_table_create(&state, identity, ident).await?;
    let principal = capability.receipt().principal.clone();
    let ident = capability.table().clone();
    let table = TableRecord::new(
        ident.clone(),
        request.location,
        request.metadata_location,
        request.metadata,
        principal.clone(),
    );
    let table = state.store.create_table(table).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.created",
            Some(ident.clone()),
            principal.clone(),
            json!({
                "event-type": "table.created",
                "table": ident,
                "authorization-receipt": capability.receipt(),
                "metadata-location": table.metadata_location,
                "location": table.location,
                "metadata-graph": table_metadata_graph_summary(&table.metadata),
                "version": table.version,
            }),
        )?)
        .await?;
    Ok(Json(load_table_response(table)))
}

async fn load_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    load_table_in_warehouse(state.warehouse.clone(), state, headers, namespace, table).await
}

async fn load_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    load_table_in_warehouse(warehouse, state, headers, namespace, table).await
}

async fn load_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_load(&state, identity, ident).await?;
    let table = state.store.load_table(capability.table()).await?;
    let ident = capability.table().clone();
    let principal = capability.receipt().principal.clone();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.loaded",
            Some(ident.clone()),
            principal.clone(),
            json!({
                "event-type": "table.loaded",
                "table": ident,
                "authorization-receipt": capability.receipt(),
                "metadata-location": table.metadata_location,
                "metadata-graph": table_metadata_graph_summary(&table.metadata),
                "version": table.version,
            }),
        )?)
        .await?;
    Ok(Json(load_table_response(table)))
}

async fn delete_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<StatusCode, LakeCatHttpError> {
    delete_table_in_warehouse(state.warehouse.clone(), state, headers, namespace, table).await
}

async fn delete_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    delete_table_in_warehouse(warehouse, state, headers, namespace, table).await
}

async fn delete_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
) -> Result<StatusCode, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_drop(&state, identity, ident).await?;
    let ident = capability.table().clone();
    state
        .store
        .soft_delete_table(
            &ident,
            capability.receipt().principal.clone(),
            Some(serde_json::to_value(capability.receipt()).map_err(|err| {
                LakeCatError::Internal(format!("failed to encode drop receipt: {err}"))
            })?),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_restore(&state, identity, ident).await?;
    let restored = state
        .store
        .restore_table(
            capability.table(),
            capability.receipt().principal.clone(),
            Some(serde_json::to_value(capability.receipt()).map_err(|err| {
                LakeCatError::Internal(format!("failed to encode restore receipt: {err}"))
            })?),
        )
        .await?;
    Ok(Json(load_table_response(restored)))
}

async fn load_credentials(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<Json<LoadCredentialsResponse>, LakeCatHttpError> {
    load_credentials_in_warehouse(state.warehouse.clone(), state, headers, namespace, table).await
}

async fn load_credentials_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<Json<LoadCredentialsResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    load_credentials_in_warehouse(warehouse, state, headers, namespace, table).await
}

async fn load_credentials_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
) -> Result<Json<LoadCredentialsResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_credentials_vend(&state, identity, ident).await?;
    let table = state.store.load_table(capability.table()).await?;
    let storage_profile = state.store.storage_profile_for_table(&table).await?;
    let read_restriction = capability.read_restriction()?;
    let raw_exception = capability
        .receipt()
        .context
        .get("lakecat:raw-credential-exception");
    let credential_block_reason = if let Some(exception) = raw_exception {
        (exception.get("allowed").and_then(Value::as_bool) == Some(false)).then(|| {
            exception
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("fine-grained read restriction requires Sail-planned reads")
        })
    } else {
        read_restriction
            .requires_governed_read()
            .then_some("fine-grained read restriction requires Sail-planned reads")
    };
    let storage_credentials = if credential_block_reason.is_some() {
        Vec::new()
    } else {
        state
            .credential_issuer
            .issue(CredentialIssuanceRequest {
                table: table.clone(),
                profile: storage_profile.clone(),
                authorization_receipt: capability.receipt().clone(),
            })
            .await?
    };
    let ident = capability.table().clone();
    let mut audit_payload = credentials_vend_audit_payload(
        &ident,
        &table,
        &storage_profile,
        storage_credentials.len(),
        capability.receipt(),
    );
    if let Some(reason) = credential_block_reason {
        audit_payload["lakecat:credential-block-reason"] = json!(reason);
    }
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "credentials.vend-attempted",
            Some(ident.clone()),
            capability.receipt().principal.clone(),
            audit_payload,
        )?)
        .await?;
    Ok(Json(LoadCredentialsResponse {
        storage_credentials,
    }))
}

fn credentials_vend_audit_payload(
    ident: &TableIdent,
    table: &TableRecord,
    storage_profile: &StorageProfile,
    credential_count: usize,
    receipt: &AuthorizationReceipt,
) -> Value {
    let mut audit_payload = json!({
        "event-type": "credentials.vend-attempted",
        "table": ident.clone(),
        "authorization-receipt": receipt,
        "storage-location": table.location,
        "storage-profile-id": storage_profile.profile_id,
        "secret-ref-present": storage_profile.secret_ref.is_some(),
        "credential-count": credential_count,
        "mode": storage_profile.issuance_mode.as_str(),
    });
    if let Some(restriction) = receipt.context.get("read-restriction") {
        audit_payload["read-restriction"] = restriction.clone();
    }
    if let Some(exception) = receipt.context.get("lakecat:raw-credential-exception") {
        audit_payload["lakecat:raw-credential-exception"] = exception.clone();
    }
    audit_payload
}

fn table_scan_planned_audit_payload(
    ident: &TableIdent,
    table: &TableRecord,
    receipt: &AuthorizationReceipt,
    scan: &lakecat_sail::ScanPlan,
) -> Value {
    let mut audit_payload = json!({
        "event-type": "table.scan-planned",
        "table": ident,
        "authorization-receipt": receipt,
        "planned-by": scan.planned_by,
        "snapshot-id": scan.snapshot_id,
        "scan-task-count": scan.scan_tasks.len(),
        "storage-location": table.location,
        "metadata-location": table.metadata_location,
    });
    if let Some(restriction) = receipt.context.get("read-restriction") {
        audit_payload["read-restriction"] = restriction.clone();
    }
    audit_payload
}

fn table_scan_tasks_fetched_audit_payload(
    ident: &TableIdent,
    table: &TableRecord,
    receipt: &AuthorizationReceipt,
    fetched: &lakecat_sail::FetchScanTasksPlan,
) -> Value {
    let mut audit_payload = json!({
        "event-type": "table.scan-tasks-fetched",
        "table": ident,
        "authorization-receipt": receipt,
        "planned-by": fetched.planned_by,
        "snapshot-id": fetched.snapshot_id,
        "plan-task": fetched.plan_task,
        "file-scan-task-count": fetched.file_scan_tasks.len(),
        "delete-file-count": fetched.delete_files.len(),
        "child-plan-task-count": fetched.plan_tasks.len(),
        "storage-location": table.location,
        "metadata-location": table.metadata_location,
    });
    if let Some(restriction) = receipt.context.get("read-restriction") {
        audit_payload["read-restriction"] = restriction.clone();
    }
    audit_payload
}

fn table_metadata_graph_summary(metadata: &Value) -> Value {
    json!({
        "current-schema-id": metadata.get("current-schema-id").cloned().unwrap_or(Value::Null),
        "fields": metadata_current_schema_fields(metadata),
        "current-snapshot-id": metadata.get("current-snapshot-id").cloned().unwrap_or(Value::Null),
        "current-snapshot": metadata_current_snapshot(metadata).cloned().unwrap_or(Value::Null),
    })
}

fn metadata_current_schema_fields(metadata: &Value) -> Vec<Value> {
    let current_schema_id = metadata.get("current-schema-id").and_then(Value::as_i64);
    metadata
        .get("schemas")
        .and_then(Value::as_array)
        .and_then(|schemas| {
            schemas
                .iter()
                .find(|schema| schema.get("schema-id").and_then(Value::as_i64) == current_schema_id)
        })
        .or_else(|| metadata.get("schema"))
        .and_then(|schema| schema.get("fields"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn metadata_current_snapshot(metadata: &Value) -> Option<&Value> {
    let current_snapshot_id = metadata
        .get("current-snapshot-id")
        .and_then(Value::as_i64)?;
    metadata
        .get("snapshots")
        .and_then(Value::as_array)
        .and_then(|snapshots| {
            snapshots.iter().find(|snapshot| {
                snapshot.get("snapshot-id").and_then(Value::as_i64) == Some(current_snapshot_id)
            })
        })
}

async fn list_storage_profiles(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
) -> Result<Json<ListStorageProfilesResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_storage_profile_manage(&state, request_identity(&headers)?).await?;
    let profiles = state.store.list_storage_profiles(&warehouse).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "storage-profile.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "storage-profile.listed",
                "warehouse": warehouse.as_str(),
                "authorization-receipt": capability.receipt(),
                "storage-profile-count": profiles.len(),
            }),
        )?)
        .await?;
    Ok(Json(ListStorageProfilesResponse {
        storage_profiles: profiles.iter().map(storage_profile_response).collect(),
    }))
}

async fn list_warehouses(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListWarehousesResponse>, LakeCatHttpError> {
    let capability = authorize_warehouse_manage(&state, request_identity(&headers)?).await?;
    let warehouses = state.store.list_warehouses().await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "warehouse.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "warehouse.listed",
                "warehouse-count": warehouses.len(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListWarehousesResponse {
        warehouses: warehouses.iter().map(warehouse_response).collect(),
    }))
}

async fn list_project_warehouses(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Result<Json<ListWarehousesResponse>, LakeCatHttpError> {
    let capability = authorize_warehouse_manage(&state, request_identity(&headers)?).await?;
    let warehouses = state.store.list_project_warehouses(&project).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "warehouse.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "warehouse.listed",
                "project-id": project.as_str(),
                "warehouse-count": warehouses.len(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListWarehousesResponse {
        warehouses: warehouses.iter().map(warehouse_response).collect(),
    }))
}

async fn list_projects(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListProjectsResponse>, LakeCatHttpError> {
    let capability = authorize_project_manage(&state, request_identity(&headers)?).await?;
    let projects = state.store.list_projects().await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "project.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "project.listed",
                "project-count": projects.len(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListProjectsResponse {
        projects: projects.iter().map(project_response).collect(),
    }))
}

async fn list_servers(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListServersResponse>, LakeCatHttpError> {
    let capability = authorize_server_manage(&state, request_identity(&headers)?).await?;
    let servers = state.store.list_servers().await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "server.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "server.listed",
                "server-count": servers.len(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListServersResponse {
        servers: servers.iter().map(server_response).collect(),
    }))
}

async fn upsert_server(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(server): Path<String>,
    Json(request): Json<UpsertServerRequest>,
) -> Result<Json<ServerResponse>, LakeCatHttpError> {
    let capability = authorize_server_manage(&state, request_identity(&headers)?).await?;
    let record = ServerRecord::new(
        server,
        request.display_name,
        request.endpoint_url,
        request.properties,
        capability.receipt().principal.clone(),
    )?;
    let record = state.store.upsert_server(record).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "server.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "server.upserted",
                "server-id": record.server_id.as_str(),
                "server-record": server_response(&record),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(server_response(&record)))
}

async fn upsert_project(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Json(request): Json<UpsertProjectRequest>,
) -> Result<Json<ProjectResponse>, LakeCatHttpError> {
    let capability = authorize_project_manage(&state, request_identity(&headers)?).await?;
    let record = ProjectRecord::new(
        project,
        request.server_id,
        request.display_name,
        request.properties,
        capability.receipt().principal.clone(),
    )?;
    let record = state.store.upsert_project(record).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "project.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "project.upserted",
                "project-id": record.project_id.as_str(),
                "project-record": project_response(&record),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(project_response(&record)))
}

async fn upsert_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
    Json(request): Json<UpsertWarehouseRequest>,
) -> Result<Json<WarehouseResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_warehouse_manage(&state, request_identity(&headers)?).await?;
    let record = WarehouseRecord::new(
        warehouse.clone(),
        request.project_id.unwrap_or_else(|| "default".to_string()),
        request.storage_root,
        request.properties,
        capability.receipt().principal.clone(),
    )?;
    let record = state.store.upsert_warehouse(record).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "warehouse.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "warehouse.upserted",
                "warehouse": warehouse.as_str(),
                "warehouse-record": warehouse_response(&record),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(warehouse_response(&record)))
}

async fn upsert_project_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((project, warehouse)): Path<(String, String)>,
    Json(request): Json<UpsertWarehouseRequest>,
) -> Result<Json<WarehouseResponse>, LakeCatHttpError> {
    if let Some(request_project) = request.project_id.as_deref()
        && request_project != project
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "warehouse project id {request_project} does not match route project {project}"
        ))
        .into());
    }
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_warehouse_manage(&state, request_identity(&headers)?).await?;
    let record = WarehouseRecord::new(
        warehouse.clone(),
        project,
        request.storage_root,
        request.properties,
        capability.receipt().principal.clone(),
    )?;
    let record = state.store.upsert_warehouse(record).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "warehouse.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "warehouse.upserted",
                "project-id": record.project_id.as_str(),
                "warehouse": warehouse.as_str(),
                "warehouse-record": warehouse_response(&record),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(warehouse_response(&record)))
}

async fn upsert_storage_profile(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, profile)): Path<(String, String)>,
    Json(request): Json<UpsertStorageProfileRequest>,
) -> Result<Json<StorageProfileResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_storage_profile_manage(&state, request_identity(&headers)?).await?;
    let storage_profile = StorageProfile::new(
        profile,
        warehouse.clone(),
        request.location_prefix,
        request.provider.parse::<StorageProvider>()?,
        request.issuance_mode.parse::<CredentialIssuanceMode>()?,
        request.secret_ref,
        request.public_config,
    )?;
    let storage_profile = state.store.upsert_storage_profile(storage_profile).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "storage-profile.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "storage-profile.upserted",
                "warehouse": warehouse.as_str(),
                "storage-profile": storage_profile_response(&storage_profile),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(storage_profile_response(&storage_profile)))
}

async fn list_views(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<Json<ListViewsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_manage(&state, request_identity(&headers)?).await?;
    let views = state.store.list_views(&warehouse, &namespace).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.listed",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view-count": views.len(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListViewsResponse {
        views: views.iter().map(view_response).collect(),
    }))
}

async fn catalog_list_views(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<Json<ListViewsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_load(&state, request_identity(&headers)?).await?;
    let views = state.store.list_views(&warehouse, &namespace).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.listed",
                "interface": "iceberg-rest",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view-count": views.len(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListViewsResponse {
        views: views.iter().map(view_response).collect(),
    }))
}

async fn catalog_load_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
) -> Result<Json<ViewResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_load(&state, request_identity(&headers)?).await?;
    let record = state
        .store
        .load_view(&warehouse, &namespace, &view_name)
        .await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.loaded",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.loaded",
                "interface": "iceberg-rest",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view": view_response(&record),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(view_response(&record)))
}

async fn upsert_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
    Json(request): Json<UpsertViewRequest>,
) -> Result<Json<ViewResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_manage(&state, request_identity(&headers)?).await?;
    let record = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new(view)?,
        request.sql,
        request.dialect,
        request.schema_version,
        request.properties,
        capability.receipt().principal.clone(),
    )?
    .with_columns(view_columns_from_request(request.columns)?)?;
    let record = state.store.upsert_view(record).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.upserted",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view": view_response(&record),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(view_response(&record)))
}

async fn drop_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_drop(&state, request_identity(&headers)?).await?;
    let record = state
        .store
        .drop_view(&warehouse, &namespace, &view_name)
        .await?;
    record_view_drop_audit(&state, &warehouse, &namespace, &record, &capability, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn catalog_upsert_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
    Json(request): Json<UpsertViewRequest>,
) -> Result<Json<ViewResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_manage(&state, request_identity(&headers)?).await?;
    let record = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new(view)?,
        request.sql,
        request.dialect,
        request.schema_version,
        request.properties,
        capability.receipt().principal.clone(),
    )?
    .with_columns(view_columns_from_request(request.columns)?)?;
    let record = state.store.upsert_view(record).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.upserted",
                "interface": "iceberg-rest",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view": view_response(&record),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(view_response(&record)))
}

async fn catalog_drop_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_drop(&state, request_identity(&headers)?).await?;
    let record = state
        .store
        .drop_view(&warehouse, &namespace, &view_name)
        .await?;
    record_view_drop_audit(
        &state,
        &warehouse,
        &namespace,
        &record,
        &capability,
        Some("iceberg-rest"),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn record_view_drop_audit(
    state: &LakeCatState,
    warehouse: &WarehouseName,
    namespace: &Namespace,
    record: &ViewRecord,
    capability: &ViewDropCapability,
    interface: Option<&str>,
) -> Result<(), LakeCatHttpError> {
    let mut payload = json!({
        "event-type": "view.dropped",
        "warehouse": warehouse.as_str(),
        "namespace": namespace.parts(),
        "view": view_response(record),
        "authorization-receipt": capability.receipt(),
    });
    if let Some(interface) = interface {
        payload["interface"] = json!(interface);
    }
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.dropped",
            None,
            capability.receipt().principal.clone(),
            payload,
        )?)
        .await?;
    Ok(())
}

async fn list_policy_bindings(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
) -> Result<Json<ListPolicyBindingsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_policy_manage(&state, request_identity(&headers)?).await?;
    let policies = state.store.list_policy_bindings(&warehouse).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "policy-binding.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "policy-binding.listed",
                "warehouse": warehouse.as_str(),
                "authorization-receipt": capability.receipt(),
                "policy-count": policies.len(),
            }),
        )?)
        .await?;
    Ok(Json(ListPolicyBindingsResponse {
        policies: policies.iter().map(policy_binding_response).collect(),
    }))
}

async fn upsert_policy_binding(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, policy)): Path<(String, String)>,
    Json(request): Json<UpsertPolicyBindingRequest>,
) -> Result<Json<PolicyBindingResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_policy_manage(&state, request_identity(&headers)?).await?;
    let namespace = request.namespace.map(Namespace::new).transpose()?;
    let table = request.table.map(TableName::new).transpose()?;
    let binding = PolicyBinding::new(
        policy,
        warehouse.clone(),
        namespace,
        table,
        request.enforced,
        request.odrl,
    )?;
    let binding = state.store.upsert_policy_binding(binding).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "policy-binding.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "policy-binding.upserted",
                "warehouse": warehouse.as_str(),
                "policy": policy_binding_response(&binding),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(policy_binding_response(&binding)))
}

async fn commit_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<CommitTableRequest>,
) -> Result<Json<CommitTableResponse>, LakeCatHttpError> {
    commit_table_in_warehouse(
        state.warehouse.clone(),
        state,
        headers,
        namespace,
        table,
        request,
    )
    .await
}

async fn commit_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
    Json(request): Json<CommitTableRequest>,
) -> Result<Json<CommitTableResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    commit_table_in_warehouse(warehouse, state, headers, namespace, table, request).await
}

async fn commit_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
    request: CommitTableRequest,
) -> Result<Json<CommitTableResponse>, LakeCatHttpError> {
    let idempotency_key = request_idempotency_key(&headers)?;
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_commit(&state, identity, ident).await?;
    let current = state.store.load_table(capability.table()).await?;
    let current_metadata_location = current.metadata_location.clone();
    let idempotency_request_hash = idempotency_key
        .as_ref()
        .map(|_| {
            content_hash_json(&json!({
                "requirements": &request.requirements,
                "updates": &request.updates,
                "metadata-location": &request.metadata_location,
                "metadata": &request.metadata,
            }))
        })
        .transpose()?;
    let commit_plan = state
        .sail
        .prepare_commit(CommitPreparationRequest {
            table: capability.table().clone(),
            principal: capability.receipt().principal.clone(),
            current_metadata_location: current_metadata_location.clone(),
            new_metadata_location: request.metadata_location,
            current_metadata: current.metadata,
            new_metadata: request.metadata,
            requirements: request.requirements,
            updates: request.updates,
        })
        .await?;
    let metadata_write = write_planned_metadata(&commit_plan).await?;
    let table = match state
        .store
        .commit_table(
            capability.table(),
            TableCommit {
                requirements: commit_plan.requirements,
                updates: commit_plan.updates,
                expected_previous_metadata_location: current_metadata_location.clone(),
                new_metadata_location: commit_plan.new_metadata_location.clone(),
                new_metadata: Some(commit_plan.new_metadata.clone()),
                idempotency_key,
                idempotency_request_hash,
                principal: capability.receipt().principal.clone(),
                authorization_receipt: Some(serde_json::to_value(capability.receipt()).map_err(
                    |err| {
                        LakeCatError::Internal(format!(
                            "failed to encode authorization receipt: {err}"
                        ))
                    },
                )?),
            },
        )
        .await
    {
        Ok(table) => table,
        Err(err) => {
            cleanup_planned_metadata(metadata_write, current_metadata_location.as_deref()).await?;
            return Err(err.into());
        }
    };
    Ok(Json(CommitTableResponse {
        metadata_location: table.metadata_location,
        metadata: table.metadata,
    }))
}

#[derive(Debug, Clone)]
struct PlannedMetadataWrite {
    location: String,
}

fn metadata_object_store(
    location: &str,
) -> Result<(Box<dyn ObjectStore>, ObjectPath), LakeCatError> {
    let url = Url::parse(location).map_err(|err| {
        LakeCatError::InvalidArgument(format!("invalid metadata location '{location}': {err}"))
    })?;
    object_store::parse_url_opts(&url, std::env::vars()).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "metadata object location '{location}' is not supported or is not configured: {err}"
        ))
    })
}

async fn write_planned_metadata(
    commit_plan: &lakecat_sail::CommitPlan,
) -> Result<Option<PlannedMetadataWrite>, LakeCatError> {
    if !commit_plan.metadata_write_required {
        return Ok(None);
    }
    let Some(location) = commit_plan.new_metadata_location.as_deref() else {
        return Ok(None);
    };
    let (store, object_path) = metadata_object_store(location)?;
    let payload = serde_json::to_vec_pretty(&commit_plan.new_metadata)
        .map_err(|err| LakeCatError::Internal(format!("failed to encode metadata JSON: {err}")))?;
    store
        .put(&object_path, PutPayload::from(payload))
        .await
        .map_err(|err| {
            LakeCatError::Internal(format!(
                "failed to write metadata object '{location}': {err}"
            ))
        })?;
    Ok(Some(PlannedMetadataWrite {
        location: location.to_string(),
    }))
}

async fn cleanup_planned_metadata(
    write: Option<PlannedMetadataWrite>,
    previous_metadata_location: Option<&str>,
) -> Result<(), LakeCatError> {
    let Some(write) = write else {
        return Ok(());
    };
    if previous_metadata_location == Some(write.location.as_str()) {
        return Ok(());
    }
    let (store, object_path) = metadata_object_store(&write.location)?;
    store.delete(&object_path).await.map_err(|err| {
        LakeCatError::Internal(format!(
            "failed to clean up uncommitted metadata object '{}': {err}",
            write.location
        ))
    })?;
    Ok(())
}

async fn plan_table_scan(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<PlanTableScanRequest>,
) -> Result<Json<PlanTableScanResponse>, LakeCatHttpError> {
    plan_table_scan_in_warehouse(
        state.warehouse.clone(),
        state,
        headers,
        namespace,
        table,
        request,
    )
    .await
}

async fn plan_table_scan_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
    Json(request): Json<PlanTableScanRequest>,
) -> Result<Json<PlanTableScanResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    plan_table_scan_in_warehouse(warehouse, state, headers, namespace, table, request).await
}

async fn plan_table_scan_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
    request: PlanTableScanRequest,
) -> Result<Json<PlanTableScanResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_scan(&state, identity, ident.clone()).await?;
    let table = state.store.load_table(capability.table()).await?;
    let (scan, scan_request_extensions) =
        plan_scan_with_capability(&state, &capability, &table, request).await?;
    let ident = capability.table().clone();
    let principal = capability.receipt().principal.clone();
    let audit_payload =
        table_scan_planned_audit_payload(&ident, &table, capability.receipt(), &scan);
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.scan-planned",
            Some(ident.clone()),
            principal.clone(),
            audit_payload,
        )?)
        .await?;
    Ok(Json(PlanTableScanResponse {
        table: TableIdentifier::from_ident(&ident),
        planned_by: scan.planned_by,
        status: "completed".to_string(),
        snapshot_id: scan.snapshot_id,
        plan_tasks: plan_task_tokens(&scan.scan_tasks),
        lakecat_plan_tasks: scan.scan_tasks,
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        residual_filter: merge_scan_request_extensions(
            scan.residual_filter,
            scan_request_extensions,
        ),
    }))
}

async fn plan_scan_with_capability(
    state: &LakeCatState,
    capability: &TableScanCapability,
    table: &TableRecord,
    request: PlanTableScanRequest,
) -> Result<(lakecat_sail::ScanPlan, serde_json::Value), LakeCatHttpError> {
    request.validate_scan_mode()?;
    #[cfg(feature = "sail-local")]
    let _ = &table;
    let requested_projection = request.projected_fields();
    let restriction = capability.read_restriction()?;
    let projection = restriction.effective_projection(&requested_projection)?;
    let filters = request.filter_values();
    let stats_fields = restriction.effective_stats_fields(&request.stats_fields);
    let scan_request_extensions = json!({
        "case-sensitive": request.case_sensitive,
        "use-snapshot-schema": request.use_snapshot_schema,
        "start-snapshot-id": request.start_snapshot_id,
        "end-snapshot-id": request.end_snapshot_id,
        "requested-projection": requested_projection,
        "effective-projection": projection,
        "read-restriction": restriction,
        "stats-fields": stats_fields,
    });
    #[cfg(feature = "sail-local")]
    let scan = {
        let provider = LakeCatCatalogProvider::new(
            "lakecat",
            capability.table().warehouse.clone(),
            state.store.clone(),
            state.sail.clone(),
            state.governance.clone(),
            capability.receipt().principal.clone(),
        );
        provider
            .plan_table_scan_for_ident(
                capability.table(),
                ProviderScanPlanningRequest {
                    projection: requested_projection,
                    filters,
                    limit: request.limit,
                    snapshot_id: request.snapshot_id,
                    start_snapshot_id: request.start_snapshot_id,
                    end_snapshot_id: request.end_snapshot_id,
                },
            )
            .await
            .map_err(catalog_provider_error)?
    };
    #[cfg(not(feature = "sail-local"))]
    let scan = state
        .sail
        .plan_scan(ScanPlanningRequest {
            table: capability.table().clone(),
            principal: capability.receipt().principal.clone(),
            metadata_location: table.metadata_location.clone(),
            table_metadata: table.metadata.clone(),
            projection,
            filters: {
                let mut filters = filters;
                if let Some(row_predicate) = restriction.row_predicate.clone() {
                    filters.push(row_predicate);
                }
                filters
            },
            limit: request.limit,
            snapshot_id: request.snapshot_id,
            start_snapshot_id: request.start_snapshot_id,
            end_snapshot_id: request.end_snapshot_id,
        })
        .await?;
    Ok((scan, scan_request_extensions))
}

#[cfg(feature = "sail-local")]
fn catalog_provider_error(error: impl std::fmt::Display) -> LakeCatHttpError {
    let message = error.to_string();
    if message.contains("invalid argument") {
        LakeCatError::InvalidArgument(message).into()
    } else if message.contains("not found") {
        LakeCatError::NotFound {
            object: "catalog object",
            name: message,
        }
        .into()
    } else if message.contains("conflict") {
        LakeCatError::Conflict(message).into()
    } else if message.contains("not supported") {
        LakeCatError::NotSupported(message).into()
    } else {
        LakeCatError::Internal(format!("LakeCat provider scan planning failed: {message}")).into()
    }
}

async fn fetch_scan_tasks(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<ApiFetchScanTasksRequest>,
) -> Result<Json<FetchScanTasksResponse>, LakeCatHttpError> {
    fetch_scan_tasks_in_warehouse(
        state.warehouse.clone(),
        state,
        headers,
        namespace,
        table,
        request,
    )
    .await
}

async fn fetch_scan_tasks_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
    Json(request): Json<ApiFetchScanTasksRequest>,
) -> Result<Json<FetchScanTasksResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    fetch_scan_tasks_in_warehouse(warehouse, state, headers, namespace, table, request).await
}

async fn fetch_scan_tasks_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
    request: ApiFetchScanTasksRequest,
) -> Result<Json<FetchScanTasksResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_scan(&state, identity, ident).await?;
    let table = state.store.load_table(capability.table()).await?;
    let fetched = fetch_scan_tasks_with_capability(&state, &capability, &table, request).await?;
    let ident = capability.table().clone();
    let audit_payload =
        table_scan_tasks_fetched_audit_payload(&ident, &table, capability.receipt(), &fetched);
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.scan-tasks-fetched",
            Some(ident.clone()),
            capability.receipt().principal.clone(),
            audit_payload,
        )?)
        .await?;
    Ok(Json(FetchScanTasksResponse {
        table: TableIdentifier::from_ident(&ident),
        planned_by: fetched.planned_by,
        plan_task: fetched.plan_task,
        snapshot_id: fetched.snapshot_id,
        file_scan_tasks: fetched.file_scan_tasks,
        delete_files: fetched.delete_files,
        plan_tasks: plan_task_tokens(&fetched.plan_tasks),
        lakecat_plan_tasks: fetched.plan_tasks,
        residual_filter: merge_fetch_scan_tasks_extensions(
            fetched.residual_filter,
            fetch_scan_tasks_extensions(capability.receipt())?,
        ),
    }))
}

async fn fetch_scan_tasks_with_capability(
    state: &LakeCatState,
    capability: &TableScanCapability,
    table: &TableRecord,
    request: ApiFetchScanTasksRequest,
) -> Result<lakecat_sail::FetchScanTasksPlan, LakeCatHttpError> {
    #[cfg(feature = "sail-local")]
    let _ = &table;
    #[cfg(not(feature = "sail-local"))]
    let restriction = capability.read_restriction()?;
    #[cfg(feature = "sail-local")]
    let fetched = {
        let provider = LakeCatCatalogProvider::new(
            "lakecat",
            capability.table().warehouse.clone(),
            state.store.clone(),
            state.sail.clone(),
            state.governance.clone(),
            capability.receipt().principal.clone(),
        );
        provider
            .fetch_table_scan_tasks_for_ident(
                capability.table(),
                ProviderFetchScanTasksRequest {
                    plan_task: request.plan_task,
                },
            )
            .await
            .map_err(catalog_provider_error)?
    };
    #[cfg(not(feature = "sail-local"))]
    let fetched = state
        .sail
        .fetch_scan_tasks(SailFetchScanTasksRequest {
            table: capability.table().clone(),
            principal: capability.receipt().principal.clone(),
            metadata_location: table.metadata_location.clone(),
            table_metadata: table.metadata.clone(),
            plan_task: request.plan_task,
            required_projection: restriction.effective_projection(&[])?,
            required_filters: restriction.mandatory_filters(),
        })
        .await?;
    Ok(fetched)
}

fn plan_task_tokens(tasks: &[serde_json::Value]) -> Vec<String> {
    tasks
        .iter()
        .filter_map(|task| task.get("plan-task").and_then(serde_json::Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn merge_scan_request_extensions(
    residual_filter: Option<serde_json::Value>,
    extensions: serde_json::Value,
) -> Option<serde_json::Value> {
    merge_lakecat_residual_extension(residual_filter, "lakecat:scan-request", extensions)
}

fn fetch_scan_tasks_extensions(
    receipt: &AuthorizationReceipt,
) -> Result<serde_json::Value, LakeCatHttpError> {
    Ok(json!({
        "read-restriction": receipt
            .context
            .get("read-restriction")
            .cloned()
            .unwrap_or_else(|| json!(ReadRestriction::unrestricted())),
    }))
}

fn merge_fetch_scan_tasks_extensions(
    residual_filter: Option<serde_json::Value>,
    extensions: serde_json::Value,
) -> Option<serde_json::Value> {
    merge_lakecat_residual_extension(residual_filter, "lakecat:fetch-scan-tasks", extensions)
}

fn merge_lakecat_residual_extension(
    residual_filter: Option<serde_json::Value>,
    extension_key: &str,
    extensions: serde_json::Value,
) -> Option<serde_json::Value> {
    match residual_filter {
        Some(mut residual @ serde_json::Value::Object(_)) => {
            residual[extension_key] = extensions;
            Some(residual)
        }
        Some(residual) => {
            let mut object = serde_json::Map::new();
            object.insert("lakecat:residual-filter".to_string(), residual);
            object.insert(extension_key.to_string(), extensions);
            Some(serde_json::Value::Object(object))
        }
        None => {
            let mut object = serde_json::Map::new();
            object.insert(extension_key.to_string(), extensions);
            Some(serde_json::Value::Object(object))
        }
    }
}

async fn querygraph_bootstrap(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<QueryGraphBootstrap>, LakeCatHttpError> {
    let capability = authorize_graph_read(&state, request_identity(&headers)?).await?;
    let tables = state.store.list_tables(&state.warehouse).await?;
    let mut table_policy_bindings = Vec::with_capacity(tables.len());
    let mut policy_binding_count = 0usize;
    for table in tables {
        let policy_bindings = state.store.policy_bindings_for_table(&table.ident).await?;
        policy_binding_count += policy_bindings.len();
        table_policy_bindings.push((table, policy_bindings));
    }
    let namespaces = state.store.list_namespaces(&state.warehouse).await?;
    let mut views = Vec::new();
    for namespace in namespaces {
        views.extend(state.store.list_views(&state.warehouse, &namespace).await?);
    }
    let bundle = QueryGraphBootstrap::from_tables_views_with_policy_bindings(
        state.warehouse.clone(),
        table_policy_bindings,
        views,
    )?;
    let verification = bundle.verify_manifest()?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "querygraph.bootstrap",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "querygraph.bootstrap",
                "authorization-receipt": capability.receipt(),
                "warehouse": state.warehouse.as_str(),
                "table-count": verification.table_count,
                "view-count": verification.view_count,
                "policy-binding-count": policy_binding_count,
                "verified-tables": verification.verified_tables,
                "verified-views": verification.verified_views,
                "bundle-hash": verification.bundle_hash,
                "graph-hash": verification.graph_hash,
                "open-lineage-hash": verification.open_lineage_hash,
                "querygraph-import-hash": verification.querygraph_import_hash,
                "table-artifacts": &bundle.manifest.table_artifacts,
                "view-artifacts": &bundle.manifest.view_artifacts,
                "standards": verification.standards,
            }),
        )?)
        .await?;
    Ok(Json(bundle))
}

async fn drain_lineage_outbox(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<LineageDrainResponse>, LakeCatHttpError> {
    let _capability = authorize_lineage_read(&state, request_identity(&headers)?).await?;
    Ok(Json(drain_outbox_once(&state, 100).await?))
}

async fn project_outbox_event(
    state: &LakeCatState,
    event: &OutboxEvent,
) -> Result<OutboxProjectionReceipt, LakeCatError> {
    let event_payload = event
        .payload
        .get("payload")
        .unwrap_or(&event.payload)
        .clone();
    let table = outbox_table(event)?;
    let principal = outbox_principal(event)?;
    let mut receipt = OutboxProjectionReceipt::default();
    if principal.kind != PrincipalKind::Anonymous {
        state
            .graph
            .emit(
                GraphEvent::principal(
                    GraphAction::Loaded,
                    &principal,
                    json!({
                        "event-type": event.event_type,
                        "principal": principal,
                    }),
                )
                .with_event_id(format!("{}:principal", event.event_id)),
            )
            .await?;
        receipt.graph_events += 1;
    }
    if let Some((graph_action, lineage_type)) = outbox_table_projection(event.event_type.as_str()) {
        if let Some(table) = table.clone() {
            state
                .graph
                .emit(
                    GraphEvent::table(graph_action.clone(), table.clone(), event_payload.clone())
                        .with_event_id(event.event_id.clone()),
                )
                .await?;
            receipt.graph_events += 1;
            project_table_metadata_graph_events(
                state,
                event,
                graph_action.clone(),
                &table,
                &event_payload,
                &mut receipt,
            )
            .await?;
            if outbox_is_scan_projection(event.event_type.as_str()) {
                state
                    .graph
                    .emit(
                        GraphEvent::scan_plan(
                            GraphAction::PlannedScan,
                            event.event_id.clone(),
                            event_payload.clone(),
                        )
                        .with_event_id(format!("{}:scan-plan", event.event_id)),
                    )
                    .await?;
                receipt.graph_events += 1;
            }
            if event.event_type == "table.commit" {
                let sequence_number = outbox_commit_sequence_number(event)?;
                state
                    .graph
                    .emit(
                        GraphEvent::commit(
                            GraphAction::Committed,
                            &table,
                            sequence_number,
                            event_payload.clone(),
                        )
                        .with_event_id(format!("{}:commit", event.event_id)),
                    )
                    .await?;
                receipt.graph_events += 1;
            }
            let lineage_receipt = state
                .lineage
                .emit(LineageEvent::new(
                    lineage_type,
                    principal,
                    Some(table),
                    event_payload,
                ))
                .await?;
            receipt.record_lineage(lineage_receipt);
        }
    } else if event.event_type == "namespace.created" || event.event_type == "namespace.dropped" {
        let (warehouse, namespace) = outbox_namespace(event, &state.warehouse)?;
        let (graph_action, lineage_type) = if event.event_type == "namespace.dropped" {
            (GraphAction::Deleted, LineageEventType::NamespaceDropped)
        } else {
            (GraphAction::Created, LineageEventType::NamespaceCreated)
        };
        state
            .graph
            .emit(
                GraphEvent::namespace(graph_action, warehouse, namespace, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                lineage_type,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "policy-binding.upserted" {
        let (warehouse, policy_id) = outbox_policy_binding(event, &state.warehouse)?;
        state
            .graph
            .emit(
                GraphEvent::policy(
                    GraphAction::Upserted,
                    warehouse,
                    policy_id,
                    event_payload.clone(),
                )
                .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
    } else if event.event_type == "project.upserted" {
        let project_id = outbox_project(event)?;
        state
            .graph
            .emit(
                GraphEvent::project(GraphAction::Upserted, project_id, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
    } else if event.event_type == "warehouse.upserted" {
        let warehouse = outbox_warehouse(event, &state.warehouse)?;
        state
            .graph
            .emit(
                GraphEvent::warehouse(GraphAction::Upserted, warehouse, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
    } else if event.event_type == "table.restored" {
        if let Some(table) = table {
            let lineage_receipt = state
                .lineage
                .emit(LineageEvent::new(
                    LineageEventType::TableRestored,
                    principal,
                    Some(table),
                    event_payload,
                ))
                .await?;
            receipt.record_lineage(lineage_receipt);
        }
    } else if event.event_type == "credentials.vend-attempted" {
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::CredentialsVendAttempted,
                principal,
                table,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "querygraph.bootstrap" {
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::QueryGraphBootstrap,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    }
    Ok(receipt)
}

async fn project_table_metadata_graph_events(
    state: &LakeCatState,
    event: &OutboxEvent,
    action: GraphAction,
    table: &TableIdent,
    event_payload: &Value,
    receipt: &mut OutboxProjectionReceipt,
) -> Result<(), LakeCatError> {
    let Some(metadata_graph) = event_payload
        .get("metadata-graph")
        .or_else(|| event_payload.get("metadata"))
    else {
        return Ok(());
    };
    for field in metadata_graph_fields(metadata_graph) {
        let Some(column_id) = metadata_field_id(&field) else {
            continue;
        };
        state
            .graph
            .emit(
                GraphEvent::column(
                    action.clone(),
                    table,
                    column_id.clone(),
                    json!({
                        "event-type": event.event_type,
                        "table": table,
                        "current-schema-id": metadata_graph.get("current-schema-id"),
                        "field": field,
                    }),
                )
                .with_event_id(format!("{}:column:{column_id}", event.event_id)),
            )
            .await?;
        receipt.graph_events += 1;
    }
    if let Some((snapshot_id, snapshot)) = metadata_graph_current_snapshot(metadata_graph) {
        state
            .graph
            .emit(
                GraphEvent::snapshot(
                    action,
                    table,
                    snapshot_id.clone(),
                    json!({
                        "event-type": event.event_type,
                        "table": table,
                        "current-snapshot-id": metadata_graph.get("current-snapshot-id"),
                        "snapshot": snapshot,
                    }),
                )
                .with_event_id(format!("{}:snapshot:{snapshot_id}", event.event_id)),
            )
            .await?;
        receipt.graph_events += 1;
    }
    Ok(())
}

fn metadata_graph_fields(metadata_graph: &Value) -> Vec<Value> {
    metadata_graph
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| metadata_current_schema_fields(metadata_graph))
}

fn metadata_field_id(field: &Value) -> Option<String> {
    field
        .get("id")
        .and_then(value_to_stable_part)
        .or_else(|| field.get("name").and_then(value_to_stable_part))
}

fn metadata_graph_current_snapshot(metadata_graph: &Value) -> Option<(String, Value)> {
    let snapshot = metadata_graph
        .get("current-snapshot")
        .filter(|snapshot| !snapshot.is_null())
        .cloned()
        .or_else(|| metadata_current_snapshot(metadata_graph).cloned())?;
    let snapshot_id = snapshot
        .get("snapshot-id")
        .and_then(value_to_stable_part)
        .or_else(|| {
            metadata_graph
                .get("current-snapshot-id")
                .and_then(value_to_stable_part)
        })?;
    Some((snapshot_id, snapshot))
}

fn value_to_stable_part(value: &Value) -> Option<String> {
    value
        .as_i64()
        .map(|value| value.to_string())
        .or_else(|| value.as_str().map(ToString::to_string))
}

fn outbox_table(event: &OutboxEvent) -> Result<Option<TableIdent>, LakeCatError> {
    event
        .payload
        .get("table")
        .filter(|table| !table.is_null())
        .map(|table| {
            serde_json::from_value(table.clone()).map_err(|err| {
                LakeCatError::Internal(format!("failed to decode outbox table: {err}"))
            })
        })
        .transpose()
}

fn outbox_namespace(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<(WarehouseName, Namespace), LakeCatError> {
    let warehouse = event
        .payload
        .get("payload")
        .and_then(|payload| payload.get("warehouse"))
        .or_else(|| event.payload.get("warehouse"))
        .and_then(serde_json::Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    let namespace = event
        .payload
        .get("payload")
        .and_then(|payload| payload.get("namespace"))
        .or_else(|| event.payload.get("namespace"))
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} is missing namespace payload",
                event.event_id
            ))
        })?;
    let namespace = match namespace {
        serde_json::Value::Array(parts) => Namespace::new(
            parts
                .iter()
                .map(|part| {
                    part.as_str().map(ToString::to_string).ok_or_else(|| {
                        LakeCatError::Internal(format!(
                            "outbox event {} namespace components must be strings",
                            event.event_id
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        serde_json::Value::String(path) => path.parse::<Namespace>()?,
        _ => {
            return Err(LakeCatError::Internal(format!(
                "outbox event {} namespace payload must be an array or string",
                event.event_id
            )));
        }
    };
    Ok((warehouse, namespace))
}

fn outbox_policy_binding(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<(WarehouseName, String), LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let policy = payload.get("policy").ok_or_else(|| {
        LakeCatError::Internal(format!(
            "outbox event {} is missing policy payload",
            event.event_id
        ))
    })?;
    let warehouse = policy
        .get("warehouse")
        .or_else(|| payload.get("warehouse"))
        .and_then(serde_json::Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    let policy_id = policy
        .get("policy-id")
        .and_then(serde_json::Value::as_str)
        .filter(|policy_id| !policy_id.is_empty())
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} policy payload is missing policy-id",
                event.event_id
            ))
        })?
        .to_string();
    Ok((warehouse, policy_id))
}

fn outbox_project(event: &OutboxEvent) -> Result<String, LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    payload
        .get("project-id")
        .or_else(|| {
            payload
                .get("project-record")
                .and_then(|record| record.get("project-id"))
        })
        .and_then(Value::as_str)
        .filter(|project_id| !project_id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} is missing project payload",
                event.event_id
            ))
        })
}

fn outbox_warehouse(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<WarehouseName, LakeCatError> {
    event
        .payload
        .get("payload")
        .and_then(|payload| payload.get("warehouse"))
        .or_else(|| event.payload.get("warehouse"))
        .and_then(Value::as_str)
        .map(WarehouseName::new)
        .transpose()
        .map(|warehouse| warehouse.unwrap_or_else(|| default_warehouse.clone()))
}

fn outbox_commit_sequence_number(event: &OutboxEvent) -> Result<u64, LakeCatError> {
    let commit = event
        .payload
        .get("payload")
        .and_then(|payload| payload.get("commit"))
        .or_else(|| event.payload.get("commit"))
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} is missing commit payload",
                event.event_id
            ))
        })?;
    commit
        .get("sequence_number")
        .or_else(|| commit.get("sequence-number"))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} commit payload is missing sequence number",
                event.event_id
            ))
        })
}

fn outbox_principal(event: &OutboxEvent) -> Result<Principal, LakeCatError> {
    for pointer in [
        "/payload/authorization-receipt/principal",
        "/authorization-receipt/principal",
        "/commit/principal",
    ] {
        if let Some(principal) = event.payload.pointer(pointer) {
            return serde_json::from_value(principal.clone()).map_err(|err| {
                LakeCatError::Internal(format!("failed to decode outbox principal: {err}"))
            });
        }
    }
    Ok(Principal::anonymous())
}

fn outbox_table_projection(event_type: &str) -> Option<(GraphAction, LineageEventType)> {
    match event_type {
        "table.created" => Some((GraphAction::Created, LineageEventType::TableCreated)),
        "table.loaded" => Some((GraphAction::Loaded, LineageEventType::TableLoaded)),
        "table.scan-planned" => {
            Some((GraphAction::PlannedScan, LineageEventType::TableScanPlanned))
        }
        "table.scan-tasks-fetched" => {
            Some((GraphAction::PlannedScan, LineageEventType::TableScanPlanned))
        }
        "table.commit" => Some((GraphAction::Committed, LineageEventType::TableCommitted)),
        "table.deleted" => Some((GraphAction::Deleted, LineageEventType::TableDeleted)),
        _ => None,
    }
}

fn outbox_is_scan_projection(event_type: &str) -> bool {
    matches!(
        event_type,
        "table.scan-planned" | "table.scan-tasks-fetched"
    )
}

fn load_table_response(table: TableRecord) -> LoadTableResponse {
    LoadTableResponse {
        identifier: TableIdentifier::from_ident(&table.ident),
        metadata_location: table.metadata_location,
        metadata: table.metadata,
        config: vec![],
    }
}

fn public_storage_credentials_for_profile(profile: &StorageProfile) -> Vec<StorageCredential> {
    let mut config = vec![ConfigEntry::new(
        "lakecat.storage-profile-id",
        profile.profile_id.clone(),
    )];
    config.extend(
        profile
            .public_config
            .iter()
            .map(|(key, value)| ConfigEntry::new(key.clone(), value.clone())),
    );
    vec![StorageCredential {
        prefix: profile.location_prefix.clone(),
        config,
    }]
}

fn storage_profile_response(profile: &StorageProfile) -> StorageProfileResponse {
    StorageProfileResponse {
        profile_id: profile.profile_id.clone(),
        warehouse: profile.warehouse.as_str().to_string(),
        location_prefix: profile.location_prefix.clone(),
        provider: profile.provider.as_str().to_string(),
        issuance_mode: profile.issuance_mode.as_str().to_string(),
        secret_ref: profile.secret_ref.clone(),
        public_config: profile.public_config.clone(),
    }
}

fn warehouse_response(record: &WarehouseRecord) -> WarehouseResponse {
    WarehouseResponse {
        warehouse: record.warehouse.as_str().to_string(),
        project_id: record.project_id.clone(),
        storage_root: record.storage_root.clone(),
        properties: record.properties.clone(),
    }
}

fn server_response(record: &ServerRecord) -> ServerResponse {
    ServerResponse {
        server_id: record.server_id.clone(),
        display_name: record.display_name.clone(),
        endpoint_url: record.endpoint_url.clone(),
        properties: record.properties.clone(),
    }
}

fn project_response(record: &ProjectRecord) -> ProjectResponse {
    ProjectResponse {
        project_id: record.project_id.clone(),
        server_id: record.server_id.clone(),
        display_name: record.display_name.clone(),
        properties: record.properties.clone(),
    }
}

fn view_response(record: &ViewRecord) -> ViewResponse {
    ViewResponse {
        warehouse: record.warehouse.as_str().to_string(),
        namespace: record.namespace.parts().to_vec(),
        name: record.name.as_str().to_string(),
        sql: record.sql.clone(),
        dialect: record.dialect.clone(),
        schema_version: record.schema_version,
        columns: record
            .columns
            .iter()
            .map(|column| ViewColumnResponse {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
                nullable: column.nullable,
                comment: column.comment.clone(),
            })
            .collect(),
        properties: record.properties.clone(),
    }
}

fn view_columns_from_request(
    columns: Vec<lakecat_api::ViewColumnRequest>,
) -> LakeCatResult<Vec<ViewColumnRecord>> {
    columns
        .into_iter()
        .map(|column| {
            let record = ViewColumnRecord {
                name: column.name,
                data_type: column.data_type,
                nullable: column.nullable,
                comment: column.comment,
            };
            record.validate()?;
            Ok(record)
        })
        .collect()
}

fn policy_binding_response(binding: &PolicyBinding) -> PolicyBindingResponse {
    PolicyBindingResponse {
        policy_id: binding.policy_id.clone(),
        warehouse: binding.warehouse.as_str().to_string(),
        namespace: binding
            .namespace
            .as_ref()
            .map(|namespace| namespace.parts().to_vec()),
        table: binding
            .table
            .as_ref()
            .map(|table| table.as_str().to_string()),
        enforced: binding.enforced,
        odrl: binding.odrl.clone(),
    }
}

fn read_restriction_from_policy_bindings(
    bindings: &[PolicyBinding],
) -> Result<ReadRestriction, LakeCatError> {
    ReadRestriction::from_odrl_policies(bindings.iter().map(|binding| &binding.odrl))
}

fn management_warehouse(
    _state: &LakeCatState,
    warehouse: String,
) -> Result<WarehouseName, LakeCatHttpError> {
    let warehouse = WarehouseName::new(warehouse)?;
    Ok(warehouse)
}

async fn prefixed_catalog_warehouse(
    state: &LakeCatState,
    warehouse: String,
) -> Result<WarehouseName, LakeCatHttpError> {
    let warehouse = WarehouseName::new(warehouse)?;
    state.store.load_warehouse(&warehouse).await?;
    Ok(warehouse)
}

#[derive(Debug, Clone)]
struct RequestIdentity {
    principal: Principal,
    envelope: Value,
    typedid_envelope: Option<String>,
}

fn request_identity(headers: &HeaderMap) -> Result<RequestIdentity, LakeCatHttpError> {
    let header = |name: &str| -> Result<Option<&str>, LakeCatError> {
        headers
            .get(name)
            .map(|value| {
                value.to_str().map_err(|_| {
                    LakeCatError::InvalidArgument(format!("invalid UTF-8 in {name} header"))
                })
            })
            .transpose()
    };

    let explicit_principal = header("x-lakecat-principal")?;
    let explicit_kind = header("x-lakecat-principal-kind")?
        .map(str::parse)
        .transpose()?;
    let agent_did = header("x-lakecat-agent-did")?;
    let explicit_typedid = header("x-lakecat-typedid")?;
    let typedid = explicit_typedid.or(agent_did);
    let typedid_proof = header("x-lakecat-typedid-proof")?;
    let typedid_envelope = header("x-lakecat-typedid-envelope")?;
    let delegation = header("x-lakecat-agent-delegation")?;
    let signed_summary = header("x-lakecat-agent-summary-signature")?;
    let authorization = header("authorization")?;

    let (principal, source, bearer_token_sha256) = if let Some(subject) = explicit_principal {
        (
            Principal::new(subject, explicit_kind.unwrap_or(PrincipalKind::Human))?,
            "x-lakecat-principal",
            None,
        )
    } else if let Some(did) = agent_did {
        (
            Principal::new(did, PrincipalKind::Agent)?,
            "x-lakecat-agent-did",
            None,
        )
    } else if let Some(did) = explicit_typedid {
        (
            Principal::new(did, PrincipalKind::Agent)?,
            "x-lakecat-typedid",
            None,
        )
    } else if let Some(authorization) = authorization {
        if let Some(token) = authorization.strip_prefix("Bearer ") {
            let token_sha256 = content_hash_bytes(token.as_bytes());
            (
                Principal::new(format!("bearer:{token_sha256}"), PrincipalKind::Service)?,
                "authorization",
                Some(token_sha256),
            )
        } else {
            return Err(LakeCatError::InvalidArgument(
                "unsupported Authorization scheme; use Bearer".to_string(),
            )
            .into());
        }
    } else {
        (Principal::anonymous(), "anonymous", None)
    };

    let envelope = json!({
        "type": "lakecat.request-identity.v1",
        "principal": principal,
        "source": source,
        "agent-did": agent_did,
        "typedid": typedid,
        "typedid-envelope-sha256": typedid_envelope
            .map(|value| content_hash_bytes(value.as_bytes())),
        "typedid-proof-sha256": typedid_proof.map(|value| content_hash_bytes(value.as_bytes())),
        "agent-delegation-sha256": delegation.map(|value| content_hash_bytes(value.as_bytes())),
        "agent-summary-signature-sha256": signed_summary
            .map(|value| content_hash_bytes(value.as_bytes())),
        "bearer-token-sha256": bearer_token_sha256,
        "attestation-state": "unverified",
        "raw-secret-material": false,
    });

    Ok(RequestIdentity {
        principal,
        envelope,
        typedid_envelope: typedid_envelope.map(ToString::to_string),
    })
}

fn request_idempotency_key(headers: &HeaderMap) -> Result<Option<String>, LakeCatHttpError> {
    let Some(value) = headers.get("x-lakecat-idempotency-key") else {
        return Ok(None);
    };
    let key = value.to_str().map_err(|_| {
        LakeCatError::InvalidArgument(
            "invalid UTF-8 in x-lakecat-idempotency-key header".to_string(),
        )
    })?;
    if key.is_empty() || key.len() > 128 {
        return Err(LakeCatError::InvalidArgument(
            "x-lakecat-idempotency-key must be 1..=128 ASCII characters".to_string(),
        )
        .into());
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Err(LakeCatError::InvalidArgument(
            "x-lakecat-idempotency-key may only contain A-Z, a-z, 0-9, '-', '_', '.', or ':'"
                .to_string(),
        )
        .into());
    }
    Ok(Some(key.to_string()))
}

async fn verify_typedid_identity(
    state: &LakeCatState,
    mut identity: RequestIdentity,
) -> Result<RequestIdentity, LakeCatHttpError> {
    let Some(envelope_json) = identity.typedid_envelope.as_deref() else {
        return Ok(identity);
    };
    let verification = state.typedid_verifier.verify(envelope_json).await?;
    if identity.principal.kind != PrincipalKind::Anonymous
        && identity.principal != verification.principal
    {
        return Err(LakeCatError::Conflict(format!(
            "TypeDID verified subject {} does not match supplied principal {}",
            verification.principal.subject, identity.principal.subject
        ))
        .into());
    }
    identity.principal = verification.principal.clone();
    identity.envelope["principal"] = json!(verification.principal);
    identity.envelope["source"] = json!("x-lakecat-typedid-envelope");
    identity.envelope["typedid"] = json!(identity.principal.subject);
    identity.envelope["attestation-state"] = json!("verified");
    identity.envelope["typedid-attestation"] = verification.attestation;
    Ok(identity)
}

async fn authorize(
    state: &LakeCatState,
    identity: RequestIdentity,
    action: CatalogAction,
    table: Option<TableIdent>,
) -> Result<AuthorizationReceipt, LakeCatHttpError> {
    let identity = verify_typedid_identity(state, identity).await?;
    let policy_bindings = if let Some(table) = table.as_ref() {
        state.store.policy_bindings_for_table(table).await?
    } else {
        Vec::new()
    };
    let read_restriction = if matches!(
        action,
        CatalogAction::TablePlanScan | CatalogAction::CredentialsVend
    ) && !policy_bindings.is_empty()
    {
        Some(read_restriction_from_policy_bindings(&policy_bindings)?)
    } else {
        None
    };
    let mut context = json!({
        "warehouse": state.warehouse.as_str(),
        "request-identity": identity.envelope,
        "policy-bindings": policy_bindings
            .iter()
            .map(policy_binding_response)
            .collect::<Vec<_>>(),
    });
    if let Some(restriction) = read_restriction {
        let raw_exception = matches!(action, CatalogAction::CredentialsVend)
            .then(|| raw_credential_exception_context(&restriction, &identity.principal));
        context["read-restriction"] = serde_json::to_value(restriction).map_err(|err| {
            LakeCatError::Internal(format!("failed to encode read restriction: {err}"))
        })?;
        if let Some(raw_exception) = raw_exception {
            context["lakecat:raw-credential-exception"] = raw_exception;
        }
    }
    let receipt = state
        .governance
        .authorize(AuthorizationRequest {
            principal: identity.principal,
            action,
            table,
            context,
        })
        .await?;
    if receipt.allowed {
        Ok(receipt.with_read_restriction_policy_hash()?)
    } else {
        Err(LakeCatError::Conflict("authorization denied".to_string()).into())
    }
}

fn raw_credential_exception_context(restriction: &ReadRestriction, principal: &Principal) -> Value {
    let governed_read_required = restriction.requires_governed_read();
    let trusted_human = principal.kind == PrincipalKind::Human;
    let allowed = !governed_read_required || trusted_human;
    json!({
        "requested": true,
        "allowed": allowed,
        "reason": if !governed_read_required {
            "restriction is compatible with short-lived credential vending"
        } else if trusted_human {
            "trusted human principal may use audited raw credential vending"
        } else {
            "fine-grained read restriction requires Sail-planned reads"
        },
    })
}

async fn authorize_table_create(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableCreateCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableCreate,
        Some(table.clone()),
    )
    .await?;
    Ok(TableCreateCapability::from_receipt(receipt, table)?)
}

async fn authorize_catalog_config(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<CatalogConfigCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::CatalogConfig, None).await?;
    Ok(CatalogConfigCapability::from_receipt(receipt)?)
}

async fn authorize_namespace_create(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceCreateCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceCreate, None).await?;
    Ok(NamespaceCreateCapability::from_receipt(receipt)?)
}

async fn authorize_namespace_list(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceListCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceList, None).await?;
    Ok(NamespaceListCapability::from_receipt(receipt)?)
}

async fn authorize_namespace_load(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceLoadCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceLoad, None).await?;
    Ok(NamespaceLoadCapability::from_receipt(receipt)?)
}

async fn authorize_namespace_drop(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceDropCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceDrop, None).await?;
    Ok(NamespaceDropCapability::from_receipt(receipt)?)
}

async fn authorize_table_load(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableLoadCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableLoad,
        Some(table.clone()),
    )
    .await?;
    Ok(TableLoadCapability::from_receipt(receipt, table)?)
}

async fn authorize_table_commit(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableCommitCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableCommit,
        Some(table.clone()),
    )
    .await?;
    Ok(TableCommitCapability::from_receipt(receipt, table)?)
}

async fn authorize_table_drop(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableDropCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableDrop,
        Some(table.clone()),
    )
    .await?;
    Ok(TableDropCapability::from_receipt(receipt, table)?)
}

async fn authorize_table_restore(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableRestoreCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableRestore,
        Some(table.clone()),
    )
    .await?;
    Ok(TableRestoreCapability::from_receipt(receipt, table)?)
}

async fn authorize_table_scan(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableScanCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TablePlanScan,
        Some(table.clone()),
    )
    .await?;
    Ok(TableScanCapability::from_receipt(receipt, table)?)
}

async fn authorize_credentials_vend(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<CredentialsVendCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::CredentialsVend,
        Some(table.clone()),
    )
    .await?;
    Ok(CredentialsVendCapability::from_receipt(receipt, table)?)
}

async fn authorize_warehouse_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<WarehouseManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::WarehouseManage, None).await?;
    Ok(WarehouseManageCapability::from_receipt(receipt)?)
}

async fn authorize_project_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ProjectManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ProjectManage, None).await?;
    Ok(ProjectManageCapability::from_receipt(receipt)?)
}

async fn authorize_server_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ServerManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ServerManage, None).await?;
    Ok(ServerManageCapability::from_receipt(receipt)?)
}

async fn authorize_storage_profile_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<StorageProfileManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::StorageProfileManage, None).await?;
    Ok(StorageProfileManageCapability::from_receipt(receipt)?)
}

async fn authorize_view_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ViewManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ViewManage, None).await?;
    Ok(ViewManageCapability::from_receipt(receipt)?)
}

async fn authorize_view_load(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ViewLoadCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ViewLoad, None).await?;
    Ok(ViewLoadCapability::from_receipt(receipt)?)
}

async fn authorize_view_drop(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<ViewDropCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::ViewDrop, None).await?;
    Ok(ViewDropCapability::from_receipt(receipt)?)
}

async fn authorize_policy_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<PolicyManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::PolicyManage, None).await?;
    Ok(PolicyManageCapability::from_receipt(receipt)?)
}

async fn authorize_graph_read(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<GraphReadCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::GraphRead, None).await?;
    Ok(GraphReadCapability::from_receipt(receipt)?)
}

async fn authorize_lineage_read(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<LineageReadCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::LineageRead, None).await?;
    Ok(LineageReadCapability::from_receipt(receipt)?)
}

#[derive(Debug)]
pub struct LakeCatHttpError(LakeCatError);

impl From<LakeCatError> for LakeCatHttpError {
    fn from(value: LakeCatError) -> Self {
        Self(value)
    }
}

impl IntoResponse for LakeCatHttpError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            LakeCatError::InvalidArgument(_) => StatusCode::BAD_REQUEST,
            LakeCatError::NotFound { .. } => StatusCode::NOT_FOUND,
            LakeCatError::Conflict(_) => StatusCode::CONFLICT,
            LakeCatError::NotSupported(_) => StatusCode::NOT_IMPLEMENTED,
            LakeCatError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(json!({
            "error": {
                "message": self.0.to_string(),
                "type": "LakeCatError",
                "code": status.as_u16()
            }
        }));
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use axum::body::Body;
    use http::{Method, Request, StatusCode};
    use lakecat_graph::GraphNodeLabel;
    use lakecat_lineage::LineageReceipt;
    use lakecat_store::{MemoryCatalogStore, OutboxEvent};
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    #[derive(Debug, Default)]
    struct RecordingGovernance {
        principals: Mutex<Vec<Principal>>,
        contexts: Mutex<Vec<serde_json::Value>>,
    }

    #[derive(Debug, Default)]
    struct RecordingGraph {
        events: Mutex<Vec<GraphEvent>>,
    }

    #[async_trait]
    impl CatalogGraphSink for RecordingGraph {
        async fn emit(&self, event: GraphEvent) -> lakecat_core::LakeCatResult<()> {
            self.events.lock().await.push(event);
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct RecordingLineage {
        events: Mutex<Vec<LineageEvent>>,
    }

    #[async_trait]
    impl LineageSink for RecordingLineage {
        async fn emit(&self, event: LineageEvent) -> lakecat_core::LakeCatResult<LineageReceipt> {
            self.events.lock().await.push(event);
            Ok(LineageReceipt {
                event_hash: "recorded".to_string(),
                open_lineage_hash: "recorded-openlineage".to_string(),
                sink: "recording".to_string(),
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingCredentialIssuer {
        requests: Mutex<Vec<CredentialIssuanceRequest>>,
    }

    #[async_trait]
    impl CredentialIssuer for RecordingCredentialIssuer {
        async fn issue(
            &self,
            request: CredentialIssuanceRequest,
        ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
            self.requests.lock().await.push(request.clone());
            if request.profile.issuance_mode == CredentialIssuanceMode::ShortLivedSecretRef {
                return Ok(vec![StorageCredential {
                    prefix: request.profile.location_prefix.clone(),
                    config: vec![
                        ConfigEntry::new("lakecat.storage-profile-id", request.profile.profile_id),
                        ConfigEntry::new("lakecat.credential-kind", "mock-short-lived"),
                        ConfigEntry::new(
                            "lakecat.authorization-principal",
                            request.authorization_receipt.principal.subject,
                        ),
                        ConfigEntry::new("aws.session-token", "temporary-test-token"),
                    ],
                }]);
            }
            Ok(public_storage_credentials_for_profile(&request.profile))
        }
    }

    #[cfg(feature = "typesec-local")]
    #[derive(Debug)]
    struct AllowCredentialIssuePolicy {
        subject: String,
        resource: String,
    }

    #[cfg(feature = "typesec-local")]
    impl typesec::PolicyEngine for AllowCredentialIssuePolicy {
        fn check(
            &self,
            subject: &typesec::SubjectId,
            action: &str,
            resource: &typesec::ResourceId,
        ) -> typesec::PolicyResult {
            if subject.as_str() == self.subject
                && action == "credentials.issue"
                && resource.as_str() == self.resource
            {
                typesec::PolicyResult::Allow
            } else {
                typesec::PolicyResult::Deny("not granted".to_string())
            }
        }
    }

    #[cfg(feature = "typesec-local")]
    #[derive(Debug, Default)]
    struct MockVaultSecretClient {
        requests: Mutex<Vec<(String, String, Option<String>)>>,
        response: Mutex<Option<serde_json::Value>>,
    }

    #[cfg(feature = "typesec-local")]
    #[async_trait]
    impl crate::typesec_credential_issuer::VaultSecretClient for MockVaultSecretClient {
        async fn read_secret(
            &self,
            url: &str,
            token: &str,
            namespace: Option<&str>,
        ) -> lakecat_core::LakeCatResult<serde_json::Value> {
            self.requests.lock().await.push((
                url.to_string(),
                token.to_string(),
                namespace.map(ToString::to_string),
            ));
            self.response.lock().await.clone().ok_or_else(|| {
                LakeCatError::InvalidArgument("mock Vault response missing".to_string())
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingOutboxStore {
        events: Mutex<Vec<OutboxEvent>>,
        delivered: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl CatalogStore for RecordingOutboxStore {
        async fn create_namespace(
            &self,
            _warehouse: &WarehouseName,
            _namespace: Namespace,
        ) -> lakecat_core::LakeCatResult<()> {
            Err(LakeCatError::NotSupported(
                "recording store does not create namespaces".to_string(),
            ))
        }

        async fn list_namespaces(
            &self,
            _warehouse: &WarehouseName,
        ) -> lakecat_core::LakeCatResult<Vec<Namespace>> {
            Err(LakeCatError::NotSupported(
                "recording store does not list namespaces".to_string(),
            ))
        }

        async fn list_tables(
            &self,
            _warehouse: &WarehouseName,
        ) -> lakecat_core::LakeCatResult<Vec<TableRecord>> {
            Err(LakeCatError::NotSupported(
                "recording store does not list tables".to_string(),
            ))
        }

        async fn create_table(
            &self,
            _table: TableRecord,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not create tables".to_string(),
            ))
        }

        async fn load_table(
            &self,
            _ident: &TableIdent,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not load tables".to_string(),
            ))
        }

        async fn commit_table(
            &self,
            _ident: &TableIdent,
            _commit: TableCommit,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not commit tables".to_string(),
            ))
        }

        async fn table_commit_records(
            &self,
            _ident: &TableIdent,
            _start_version: u64,
            _end_version: Option<u64>,
        ) -> lakecat_core::LakeCatResult<Vec<lakecat_store::TableCommitRecord>> {
            Err(LakeCatError::NotSupported(
                "recording store does not list table commits".to_string(),
            ))
        }

        async fn soft_delete_table(
            &self,
            _ident: &TableIdent,
            _principal: Principal,
            _authorization_receipt: Option<serde_json::Value>,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not delete tables".to_string(),
            ))
        }

        async fn restore_table(
            &self,
            _ident: &TableIdent,
            _principal: Principal,
            _authorization_receipt: Option<serde_json::Value>,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not restore tables".to_string(),
            ))
        }

        async fn pending_outbox_events(
            &self,
            sink: Option<&str>,
            limit: usize,
        ) -> lakecat_core::LakeCatResult<Vec<OutboxEvent>> {
            let events = self.events.lock().await;
            Ok(events
                .iter()
                .filter(|event| sink.is_none_or(|sink| event.sink == sink))
                .take(limit)
                .cloned()
                .collect())
        }

        async fn mark_outbox_delivered(
            &self,
            event_ids: &[String],
        ) -> lakecat_core::LakeCatResult<usize> {
            self.delivered.lock().await.extend_from_slice(event_ids);
            Ok(event_ids.len())
        }
    }

    #[async_trait]
    impl GovernanceEngine for RecordingGovernance {
        async fn authorize(
            &self,
            request: AuthorizationRequest,
        ) -> lakecat_core::LakeCatResult<lakecat_security::AuthorizationReceipt> {
            self.principals.lock().await.push(request.principal.clone());
            self.contexts.lock().await.push(request.context.clone());
            Ok(lakecat_security::AuthorizationReceipt {
                principal: request.principal,
                action: request.action,
                table: request.table,
                allowed: true,
                engine: "recording".to_string(),
                policy_hash: None,
                context: request.context,
                checked_at: chrono::Utc::now(),
            })
        }
    }

    #[test]
    fn request_identity_hashes_typedid_envelope_material() {
        let mut headers = HeaderMap::new();
        headers.insert("x-lakecat-agent-did", "did:example:agent".parse().unwrap());
        headers.insert(
            "x-lakecat-typedid-envelope",
            r#"{"protected":"typedid-envelope"}"#.parse().unwrap(),
        );
        headers.insert("x-lakecat-typedid-proof", "signed-proof".parse().unwrap());
        headers.insert(
            "x-lakecat-agent-delegation",
            "delegation-token".parse().unwrap(),
        );
        headers.insert(
            "x-lakecat-agent-summary-signature",
            "summary-secret".parse().unwrap(),
        );

        let identity = request_identity(&headers).expect("identity should parse");

        assert_eq!(identity.principal.subject, "did:example:agent");
        assert_eq!(identity.principal.kind, PrincipalKind::Agent);
        assert_eq!(
            identity.envelope["typedid-proof-sha256"],
            serde_json::json!(content_hash_bytes("signed-proof".as_bytes()))
        );
        assert_eq!(
            identity.envelope["typedid-envelope-sha256"],
            serde_json::json!(content_hash_bytes(
                r#"{"protected":"typedid-envelope"}"#.as_bytes()
            ))
        );
        assert_eq!(
            identity.envelope["agent-delegation-sha256"],
            serde_json::json!(content_hash_bytes("delegation-token".as_bytes()))
        );
        assert_eq!(
            identity.envelope["agent-summary-signature-sha256"],
            serde_json::json!(content_hash_bytes("summary-secret".as_bytes()))
        );
        assert_eq!(
            identity.envelope["raw-secret-material"],
            serde_json::json!(false)
        );
        let envelope = identity.envelope.to_string();
        assert!(!envelope.contains("signed-proof"));
        assert!(!envelope.contains("protected"));
        assert!(!envelope.contains("delegation-token"));
        assert!(!envelope.contains("summary-secret"));
    }

    #[cfg(feature = "typesec-local")]
    #[tokio::test]
    async fn typesec_typedid_envelope_verification_updates_authorization_context() {
        use typesec::integrations::{
            DidMessageBody, StaticDidResolver, TypeDidConversation, TypeDidMode, TypeDidProfile,
        };
        use typesec::{Did, DidEnvelope, Ed25519DidKey, Ed25519DidKeyStore, TypeDidGateway};

        let agent_key = Ed25519DidKey::from_seed(b"lakecat-agent-ed25519");
        let lakecat_key = Ed25519DidKey::from_seed(b"lakecat-service-ed25519");
        let agent = Did::key(agent_key.signing_public());
        let lakecat = Did::key(lakecat_key.signing_public());
        let resolver = StaticDidResolver::new()
            .with_document(agent_key.document(agent.clone()))
            .with_document(lakecat_key.document(lakecat.clone()));
        let keys = Ed25519DidKeyStore::new()
            .with_key(agent.clone(), agent_key)
            .with_key(lakecat.clone(), lakecat_key);
        let envelope = DidEnvelope::typedid(
            "lakecat-typedid-1",
            agent.clone(),
            lakecat.clone(),
            DidMessageBody::agent_message("lakecat:catalog:config", "internal"),
            TypeDidConversation::new(
                "lakecat-config",
                TypeDidMode::RequestReply,
                TypeDidProfile::ed25519_x25519_chacha20().id,
                "https",
            ),
            b"secret agent payload",
            &resolver,
            &keys,
        )
        .expect("typedid envelope");
        let envelope_json = serde_json::to_string(&envelope).expect("typedid envelope json");
        let envelope_signature = envelope.signature.clone();
        let gateway = Arc::new(TypeDidGateway::new(
            Arc::new(resolver),
            Arc::new(keys),
            lakecat,
        ));
        let governance = Arc::new(RecordingGovernance::default());
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        )
        .with_integrations(
            default_sail_engine(),
            governance.clone(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        )
        .with_typedid_verifier(crate::typesec_typedid::TypeSecTypeDidVerifier::new(gateway)));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/config")
                    .header("x-lakecat-typedid-envelope", envelope_json.clone())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let principals = governance.principals.lock().await;
        assert_eq!(principals[0].subject, agent.to_string());
        assert_eq!(principals[0].kind, PrincipalKind::Agent);
        drop(principals);

        let contexts = governance.contexts.lock().await;
        let request_identity = &contexts[0]["request-identity"];
        assert_eq!(
            request_identity["source"],
            serde_json::json!("x-lakecat-typedid-envelope")
        );
        assert_eq!(
            request_identity["typedid"],
            serde_json::json!(agent.to_string())
        );
        assert_eq!(
            request_identity["attestation-state"],
            serde_json::json!("verified")
        );
        assert_eq!(
            request_identity["typedid-envelope-sha256"],
            serde_json::json!(content_hash_bytes(envelope_json.as_bytes()))
        );
        assert_eq!(
            request_identity["typedid-attestation"]["subject"],
            serde_json::json!(agent.to_string())
        );
        assert_eq!(
            request_identity["typedid-attestation"]["envelope_id"],
            serde_json::json!("lakecat-typedid-1")
        );
        assert_eq!(
            request_identity["typedid-attestation"]["resource"],
            serde_json::json!("lakecat:catalog:config")
        );
        let rendered = request_identity.to_string();
        assert!(!rendered.contains("secret agent payload"));
        assert!(!rendered.contains(&envelope_signature));
    }

    #[test]
    fn request_identity_typedid_header_alone_selects_agent_principal() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-lakecat-typedid",
            "did:example:typedid-only".parse().unwrap(),
        );
        let identity = request_identity(&headers).expect("identity should parse");
        assert_eq!(identity.principal.subject, "did:example:typedid-only");
        assert_eq!(identity.principal.kind, PrincipalKind::Agent);
        assert_eq!(
            identity.envelope["source"],
            serde_json::json!("x-lakecat-typedid")
        );
        assert_eq!(
            identity.envelope["typedid"],
            serde_json::json!("did:example:typedid-only")
        );
    }

    #[test]
    fn request_identity_agent_did_takes_precedence_over_typedid() {
        let mut headers = HeaderMap::new();
        headers.insert("x-lakecat-agent-did", "did:example:agent".parse().unwrap());
        headers.insert("x-lakecat-typedid", "did:example:typedid".parse().unwrap());
        let identity = request_identity(&headers).expect("identity should parse");
        assert_eq!(identity.principal.subject, "did:example:agent");
        assert_eq!(identity.principal.kind, PrincipalKind::Agent);
        assert_eq!(
            identity.envelope["source"],
            serde_json::json!("x-lakecat-agent-did")
        );
    }

    #[tokio::test]
    async fn config_endpoint_reports_lakecat_capabilities() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn list_namespaces_does_not_fabricate_default_namespace() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/namespaces")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["namespaces"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn namespaces_load_and_drop_through_catalog_routes() {
        let store = MemoryCatalogStore::new();
        store
            .upsert_project(
                ProjectRecord::new(
                    "default",
                    None,
                    Some("Default Project".to_string()),
                    std::collections::BTreeMap::new(),
                    Principal::anonymous(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        store
            .upsert_warehouse(
                WarehouseRecord::new(
                    WarehouseName::new("local").unwrap(),
                    "default",
                    Some("file:///tmp/lakecat".to_string()),
                    std::collections::BTreeMap::new(),
                    Principal::anonymous(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            store,
        ));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/namespaces/empty")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/catalog/v1/namespaces")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"namespace":["empty"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/namespaces/empty")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["namespace"], serde_json::json!(["empty"]));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/catalog/v1/namespaces/empty")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/namespaces/empty")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/catalog/v1/local/namespaces")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"namespace":["prefixed"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/local/namespaces/prefixed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/catalog/v1/local/namespaces/prefixed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/catalog/v1/namespaces/default/tables")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name":"events","location":"file:///tmp/events","metadata":{"format-version":3}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/catalog/v1/namespaces/default")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn authorization_headers_resolve_typed_principal() {
        let governance = Arc::new(RecordingGovernance::default());
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        )
        .with_integrations(
            default_sail_engine(),
            governance.clone(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ));
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/config")
                    .header("x-lakecat-principal", "alice@example.com")
                    .header("x-lakecat-principal-kind", "human")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let principals = governance.principals.lock().await;
        assert_eq!(principals[0].subject, "alice@example.com");
        assert_eq!(principals[0].kind, PrincipalKind::Human);
        drop(principals);
        let contexts = governance.contexts.lock().await;
        assert_eq!(
            contexts[0]["request-identity"]["source"],
            serde_json::json!("x-lakecat-principal")
        );
        assert_eq!(
            contexts[0]["request-identity"]["principal"]["subject"],
            serde_json::json!("alice@example.com")
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_table_events_to_sinks() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal {
            subject: "agent:writer".to_string(),
            kind: PrincipalKind::Agent,
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![
                OutboxEvent {
                    event_id: "evt-namespace".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "namespace.created".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-namespace",
                        "event-type": "namespace.created",
                        "payload": {
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "namespace-create",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                            },
                            "warehouse": "local",
                            "namespace": ["default"],
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-policy".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "policy-binding.upserted".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-policy",
                        "event-type": "policy-binding.upserted",
                        "payload": {
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "policy-manage",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                            },
                            "warehouse": "local",
                            "policy": {
                                "policy-id": "agent-read",
                                "warehouse": "local",
                                "namespace": ["default"],
                                "table": "events",
                                "enforced": true,
                                "odrl": {
                                    "uid": "policy:agent-read",
                                    "lakecat:read-restriction": {
                                        "allowed-columns": ["event_id"]
                                    }
                                }
                            }
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-1".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "table.created".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-1",
                        "event-type": "table.created",
                        "table": table,
                        "payload": {
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "table-create",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                            },
                            "metadata-location": "file:///tmp/events/metadata/00000.json",
                            "metadata-graph": {
                                "current-schema-id": 1,
                                "fields": [
                                    {"id": 1, "name": "event_id", "type": "string", "required": true},
                                    {"id": 2, "name": "payload", "type": "string", "required": false}
                                ],
                                "current-snapshot-id": 42,
                                "current-snapshot": {
                                    "snapshot-id": 42,
                                    "sequence-number": 7,
                                    "timestamp-ms": 1710000000000_i64,
                                    "manifest-list": "file:///tmp/events/metadata/snap-42.avro",
                                    "summary": {"operation": "append"},
                                    "schema-id": 1
                                }
                            },
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-2".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "table.scan-tasks-fetched".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-2",
                        "event-type": "table.scan-tasks-fetched",
                        "table": table,
                        "payload": {
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "table-plan-scan",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                            },
                            "read-restriction": {
                                "allowed-columns": ["event_id"]
                            },
                            "storage-location": "file:///tmp/events",
                            "metadata-location": "file:///tmp/events/metadata/00000.json",
                        },
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-commit".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "table.commit".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-commit",
                        "event-type": "table.commit",
                        "table": table,
                        "commit": {
                            "table": table,
                            "previous_metadata_location": "file:///tmp/events/metadata/00000.json",
                            "new_metadata_location": "file:///tmp/events/metadata/00001.json",
                            "sequence_number": 7,
                            "principal": principal,
                            "request_hash": "sha256:request",
                            "idempotency_key_sha256": "sha256:idempotency",
                            "committed_at": chrono::Utc::now(),
                        },
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "table-commit",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-credentials".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "credentials.vend-attempted".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-credentials",
                        "event-type": "credentials.vend-attempted",
                        "table": table,
                        "payload": {
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "credentials-vend",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": "sha256:policy",
                                "checked_at": chrono::Utc::now(),
                                "context": {
                                    "lakecat:raw-credential-exception": {
                                        "requested": true,
                                        "allowed": false,
                                        "reason": "fine-grained read restriction requires Sail-planned reads"
                                    }
                                }
                            },
                            "credential-count": 0,
                            "lakecat:credential-block-reason": "fine-grained read restriction requires Sail-planned reads",
                            "lakecat:raw-credential-exception": {
                                "requested": true,
                                "allowed": false,
                                "reason": "fine-grained read restriction requires Sail-planned reads"
                            }
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-3".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "querygraph.bootstrap".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-3",
                        "event-type": "querygraph.bootstrap",
                        "payload": {
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "graph-read",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                                "request-identity": {
                                    "attestation-state": "verified",
                                    "agent-delegation-sha256": "sha256:delegation",
                                    "agent-summary-signature-sha256": "sha256:summary",
                                    "typedid": "did:example:agent"
                                }
                            },
                            "warehouse": "local",
                            "table-count": 1,
                            "policy-binding-count": 1,
                            "verified-tables": ["local.default.events"],
                            "verified-views": ["lakecat:view:local:default:active_customers"],
                            "bundle-hash": "sha256:bundle",
                            "graph-hash": "sha256:graph",
                            "open-lineage-hash": "sha256:openlineage",
                            "querygraph-import-hash": "sha256:querygraph-import",
                            "table-artifacts": [{
                                "stable-id": "local.default.events",
                                "croissant-hash": "sha256:croissant",
                                "cdif-hash": "sha256:cdif",
                                "osi-hash": "sha256:osi",
                                "odrl-hash": "sha256:odrl",
                                "policy-bindings-hash": "sha256:policies"
                            }],
                            "view-artifacts": [{
                                "stable-id": "lakecat:view:local:default:active_customers",
                                "osi-hash": "sha256:view-osi"
                            }],
                            "standards": ["OpenLineage", "Grust catalog graph"]
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-namespace-drop".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "namespace.dropped".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-namespace-drop",
                        "event-type": "namespace.dropped",
                        "payload": {
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "namespace-drop",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                            },
                            "warehouse": "local",
                            "namespace": ["archived"],
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
            ]),
            delivered: Mutex::default(),
        });
        let graph = Arc::new(RecordingGraph::default());
        let lineage = Arc::new(RecordingLineage::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_integrations(
                default_sail_engine(),
                AllowAllGovernanceEngine::new(),
                graph.clone(),
                lineage.clone(),
            );

        let drain = drain_outbox_once(&state, 10).await.unwrap();
        assert_eq!(drain.delivered, 8);
        assert_eq!(
            drain.event_types,
            vec![
                "namespace.created".to_string(),
                "policy-binding.upserted".to_string(),
                "table.created".to_string(),
                "table.scan-tasks-fetched".to_string(),
                "table.commit".to_string(),
                "credentials.vend-attempted".to_string(),
                "querygraph.bootstrap".to_string(),
                "namespace.dropped".to_string()
            ]
        );
        assert_eq!(drain.graph_events, 19);
        assert_eq!(drain.lineage_events, 7);
        let credential_summary = drain
            .events
            .iter()
            .find(|event| event.event_type == "credentials.vend-attempted")
            .expect("credential replay summary should be exposed");
        assert_eq!(
            credential_summary.principal_subject.as_deref(),
            Some("agent:writer")
        );
        assert_eq!(credential_summary.principal_kind.as_deref(), Some("agent"));
        assert_eq!(credential_summary.graph_events, 1);
        assert_eq!(credential_summary.lineage_events, 1);
        assert_eq!(credential_summary.credential_count, Some(0));
        assert_eq!(
            credential_summary.credential_block_reason.as_deref(),
            Some("fine-grained read restriction requires Sail-planned reads")
        );
        assert_eq!(
            credential_summary.raw_credential_exception_allowed,
            Some(false)
        );
        assert_eq!(
            credential_summary
                .raw_credential_exception_reason
                .as_deref(),
            Some("fine-grained read restriction requires Sail-planned reads")
        );
        assert_eq!(
            credential_summary.replay_event_hashes,
            vec!["recorded".to_string()]
        );
        assert_eq!(
            credential_summary.replay_open_lineage_hashes,
            vec!["recorded-openlineage".to_string()]
        );
        let bootstrap_summary = drain
            .events
            .iter()
            .find(|event| event.event_type == "querygraph.bootstrap")
            .expect("bootstrap replay summary should be exposed");
        assert_eq!(
            bootstrap_summary.principal_subject.as_deref(),
            Some("agent:writer")
        );
        assert_eq!(bootstrap_summary.principal_kind.as_deref(), Some("agent"));
        assert!(
            bootstrap_summary
                .authorization_receipt_hash
                .as_deref()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert_eq!(
            bootstrap_summary.request_identity_state.as_deref(),
            Some("verified")
        );
        assert!(
            bootstrap_summary
                .agent_delegation_hash
                .as_deref()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert!(
            bootstrap_summary
                .agent_summary_signature_hash
                .as_deref()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert_eq!(bootstrap_summary.graph_events, 1);
        assert_eq!(bootstrap_summary.lineage_events, 1);
        assert_eq!(
            bootstrap_summary.bundle_hash.as_deref(),
            Some("sha256:bundle")
        );
        assert_eq!(
            bootstrap_summary.graph_hash.as_deref(),
            Some("sha256:graph")
        );
        assert_eq!(
            bootstrap_summary.open_lineage_hash.as_deref(),
            Some("sha256:openlineage")
        );
        assert_eq!(
            bootstrap_summary.querygraph_import_hash.as_deref(),
            Some("sha256:querygraph-import")
        );
        assert_eq!(bootstrap_summary.table_artifact_count, 1);
        assert_eq!(bootstrap_summary.view_artifact_count, 1);
        assert_eq!(bootstrap_summary.policy_binding_count, 1);
        assert_eq!(
            bootstrap_summary.standards,
            vec!["OpenLineage".to_string(), "Grust catalog graph".to_string()]
        );
        assert_eq!(
            bootstrap_summary.replay_event_hashes,
            vec!["recorded".to_string()]
        );
        assert_eq!(
            bootstrap_summary.replay_open_lineage_hashes,
            vec!["recorded-openlineage".to_string()]
        );

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 19);
        assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
        assert_eq!(graph_events[0].subject, "lakecat:principal:agent:writer");
        assert_eq!(
            graph_events[0].event_id.as_deref(),
            Some("evt-namespace:principal")
        );
        assert_eq!(
            graph_events[0].properties["principal"]["kind"],
            serde_json::json!("agent")
        );
        assert_eq!(graph_events[1].label, GraphNodeLabel::Namespace);
        assert_eq!(
            graph_events[1].subject,
            "lakecat:warehouse:local:namespace:default"
        );
        assert_eq!(graph_events[1].event_id.as_deref(), Some("evt-namespace"));
        assert_eq!(
            graph_events[1].properties["authorization-receipt"]["principal"]["subject"],
            serde_json::json!("agent:writer")
        );
        assert_eq!(graph_events[2].label, GraphNodeLabel::Principal);
        assert_eq!(
            graph_events[2].event_id.as_deref(),
            Some("evt-policy:principal")
        );
        assert_eq!(graph_events[3].label, GraphNodeLabel::Policy);
        assert_eq!(graph_events[3].action, GraphAction::Upserted);
        assert_eq!(
            graph_events[3].subject,
            "lakecat:warehouse:local:policy:agent-read"
        );
        assert_eq!(graph_events[3].event_id.as_deref(), Some("evt-policy"));
        assert_eq!(
            graph_events[3].properties["policy"]["odrl"]["uid"],
            serde_json::json!("policy:agent-read")
        );
        assert_eq!(graph_events[4].label, GraphNodeLabel::Principal);
        assert_eq!(graph_events[5].label, GraphNodeLabel::Table);
        assert_eq!(graph_events[5].action, GraphAction::Created);
        assert_eq!(graph_events[5].event_id.as_deref(), Some("evt-1"));
        assert_eq!(graph_events[6].label, GraphNodeLabel::Column);
        assert_eq!(
            graph_events[6].subject,
            "lakecat:column:lakecat:table:local:default:events:1"
        );
        assert_eq!(graph_events[6].event_id.as_deref(), Some("evt-1:column:1"));
        assert_eq!(
            graph_events[6].properties["field"]["name"],
            serde_json::json!("event_id")
        );
        assert_eq!(graph_events[7].label, GraphNodeLabel::Column);
        assert_eq!(
            graph_events[7].subject,
            "lakecat:column:lakecat:table:local:default:events:2"
        );
        assert_eq!(graph_events[8].label, GraphNodeLabel::Snapshot);
        assert_eq!(
            graph_events[8].subject,
            "lakecat:snapshot:lakecat:table:local:default:events:42"
        );
        assert_eq!(
            graph_events[8].event_id.as_deref(),
            Some("evt-1:snapshot:42")
        );
        assert_eq!(
            graph_events[8].properties["snapshot"]["manifest-list"],
            serde_json::json!("file:///tmp/events/metadata/snap-42.avro")
        );
        assert_eq!(graph_events[9].label, GraphNodeLabel::Principal);
        assert_eq!(graph_events[10].label, GraphNodeLabel::Table);
        assert_eq!(graph_events[10].action, GraphAction::PlannedScan);
        assert_eq!(
            graph_events[10].properties["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(graph_events[11].label, GraphNodeLabel::ScanPlan);
        assert_eq!(graph_events[11].subject, "lakecat:scan-plan:evt-2");
        assert_eq!(
            graph_events[11].event_id.as_deref(),
            Some("evt-2:scan-plan")
        );
        assert_eq!(
            graph_events[11].properties["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(graph_events[12].label, GraphNodeLabel::Principal);
        assert_eq!(graph_events[13].label, GraphNodeLabel::Table);
        assert_eq!(graph_events[13].action, GraphAction::Committed);
        assert_eq!(graph_events[13].event_id.as_deref(), Some("evt-commit"));
        assert_eq!(
            graph_events[13].properties["commit"]["new_metadata_location"],
            serde_json::json!("file:///tmp/events/metadata/00001.json")
        );
        assert_eq!(graph_events[14].label, GraphNodeLabel::Commit);
        assert_eq!(
            graph_events[14].subject,
            "lakecat:commit:lakecat:table:local:default:events:7"
        );
        assert_eq!(
            graph_events[14].event_id.as_deref(),
            Some("evt-commit:commit")
        );
        assert_eq!(
            graph_events[14].properties["commit"]["idempotency_key_sha256"],
            serde_json::json!("sha256:idempotency")
        );
        assert_eq!(graph_events[15].label, GraphNodeLabel::Principal);
        assert_eq!(
            graph_events[15].event_id.as_deref(),
            Some("evt-credentials:principal")
        );
        assert_eq!(graph_events[16].label, GraphNodeLabel::Principal);
        assert_eq!(
            graph_events[16].event_id.as_deref(),
            Some("evt-3:principal")
        );
        assert_eq!(graph_events[17].label, GraphNodeLabel::Principal);
        assert_eq!(
            graph_events[17].event_id.as_deref(),
            Some("evt-namespace-drop:principal")
        );
        assert_eq!(graph_events[18].label, GraphNodeLabel::Namespace);
        assert_eq!(graph_events[18].action, GraphAction::Deleted);
        assert_eq!(
            graph_events[18].subject,
            "lakecat:warehouse:local:namespace:archived"
        );
        assert_eq!(
            graph_events[18].event_id.as_deref(),
            Some("evt-namespace-drop")
        );
        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 7);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::NamespaceCreated
        );
        assert_eq!(lineage_events[1].event_type, LineageEventType::TableCreated);
        assert_eq!(
            lineage_events[2].event_type,
            LineageEventType::TableScanPlanned
        );
        assert_eq!(
            lineage_events[2].payload["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            lineage_events[3].event_type,
            LineageEventType::TableCommitted
        );
        assert_eq!(
            lineage_events[3].payload["commit"]["new_metadata_location"],
            serde_json::json!("file:///tmp/events/metadata/00001.json")
        );
        assert_eq!(
            lineage_events[4].event_type,
            LineageEventType::CredentialsVendAttempted
        );
        assert_eq!(
            lineage_events[4].payload["credential-count"],
            serde_json::json!(0)
        );
        assert_eq!(
            lineage_events[4].payload["lakecat:raw-credential-exception"]["allowed"],
            serde_json::json!(false)
        );
        assert_eq!(
            lineage_events[5].event_type,
            LineageEventType::QueryGraphBootstrap
        );
        assert_eq!(
            lineage_events[5].payload["authorization-receipt"]["request-identity"]["attestation-state"],
            serde_json::json!("verified")
        );
        assert_eq!(
            lineage_events[5].payload["bundle-hash"],
            serde_json::json!("sha256:bundle")
        );
        assert_eq!(
            lineage_events[5].payload["graph-hash"],
            serde_json::json!("sha256:graph")
        );
        assert_eq!(
            lineage_events[5].payload["open-lineage-hash"],
            serde_json::json!("sha256:openlineage")
        );
        assert_eq!(
            lineage_events[5].payload["querygraph-import-hash"],
            serde_json::json!("sha256:querygraph-import")
        );
        assert_eq!(
            lineage_events[5].payload["table-artifacts"][0]["cdif-hash"],
            serde_json::json!("sha256:cdif")
        );
        assert_eq!(
            lineage_events[5].payload["view-artifacts"][0]["stable-id"],
            serde_json::json!("lakecat:view:local:default:active_customers")
        );
        assert_eq!(
            lineage_events[6].event_type,
            LineageEventType::NamespaceDropped
        );
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &[
                "evt-namespace".to_string(),
                "evt-policy".to_string(),
                "evt-1".to_string(),
                "evt-2".to_string(),
                "evt-commit".to_string(),
                "evt-credentials".to_string(),
                "evt-3".to_string(),
                "evt-namespace-drop".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_warehouse_upserts_to_graph() {
        let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-warehouse".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "warehouse.upserted".to_string(),
                payload: json!({
                    "audit-event-id": "audit-warehouse",
                    "event-type": "warehouse.upserted",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "warehouse-manage",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "warehouse": "local",
                        "warehouse-record": {
                            "warehouse": "local",
                            "project-id": "default",
                            "storage-root": "file:///tmp/lakecat",
                            "properties": {"region": "local"}
                        }
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            }]),
            delivered: Mutex::default(),
        });
        let graph = Arc::new(RecordingGraph::default());
        let lineage = Arc::new(RecordingLineage::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_integrations(
                default_sail_engine(),
                AllowAllGovernanceEngine::new(),
                graph.clone(),
                lineage,
            );

        let drain = drain_outbox_once(&state, 10).await.unwrap();
        assert_eq!(drain.delivered, 1);
        assert_eq!(drain.event_types, vec!["warehouse.upserted".to_string()]);
        assert_eq!(drain.graph_events, 2);
        assert_eq!(drain.lineage_events, 0);
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &["evt-warehouse".to_string()]
        );

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 2);
        assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
        assert_eq!(graph_events[0].subject, "lakecat:principal:agent:operator");
        assert_eq!(graph_events[1].label, GraphNodeLabel::Warehouse);
        assert_eq!(graph_events[1].subject, "lakecat:warehouse:local");
        assert_eq!(graph_events[1].event_id.as_deref(), Some("evt-warehouse"));
        assert_eq!(
            graph_events[1].properties["warehouse-record"]["project-id"],
            serde_json::json!("default")
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_project_upserts_to_graph() {
        let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-project".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "project.upserted".to_string(),
                payload: json!({
                    "audit-event-id": "audit-project",
                    "event-type": "project.upserted",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "project-manage",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "project-id": "default",
                        "project-record": {
                            "project-id": "default",
                            "display-name": "Default Project",
                            "properties": {"owner": "querygraph"}
                        }
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            }]),
            delivered: Mutex::default(),
        });
        let graph = Arc::new(RecordingGraph::default());
        let lineage = Arc::new(RecordingLineage::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_integrations(
                default_sail_engine(),
                AllowAllGovernanceEngine::new(),
                graph.clone(),
                lineage,
            );

        let drain = drain_outbox_once(&state, 10).await.unwrap();
        assert_eq!(drain.delivered, 1);
        assert_eq!(drain.event_types, vec!["project.upserted".to_string()]);
        assert_eq!(drain.graph_events, 2);
        assert_eq!(drain.lineage_events, 0);
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &["evt-project".to_string()]
        );

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 2);
        assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
        assert_eq!(graph_events[0].subject, "lakecat:principal:agent:operator");
        assert_eq!(graph_events[1].label, GraphNodeLabel::Project);
        assert_eq!(graph_events[1].subject, "lakecat:project:default");
        assert_eq!(graph_events[1].event_id.as_deref(), Some("evt-project"));
        assert_eq!(
            graph_events[1].properties["project-record"]["display-name"],
            serde_json::json!("Default Project")
        );
    }

    #[tokio::test]
    async fn lineage_drain_endpoint_replays_querygraph_bootstrap_outbox() {
        let principal = Principal {
            subject: "did:example:agent".to_string(),
            kind: PrincipalKind::Agent,
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-bootstrap".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "querygraph.bootstrap".to_string(),
                payload: json!({
                    "audit-event-id": "audit-bootstrap",
                    "event-type": "querygraph.bootstrap",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "graph-read",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                            "request-identity": {
                                "attestation-state": "verified",
                                "agent-delegation-sha256": "sha256:delegation",
                                "agent-summary-signature-sha256": "sha256:summary",
                                "typedid": "did:example:agent"
                            }
                        },
                        "warehouse": "local",
                        "table-count": 1,
                        "policy-binding-count": 1,
                        "verified-tables": ["local.default.events"],
                        "verified-views": ["lakecat:view:local:default:active_customers"],
                        "bundle-hash": "sha256:bundle",
                        "graph-hash": "sha256:graph",
                        "open-lineage-hash": "sha256:openlineage",
                        "querygraph-import-hash": "sha256:querygraph-import",
                        "table-artifacts": [{
                            "stable-id": "local.default.events",
                            "croissant-hash": "sha256:croissant",
                            "cdif-hash": "sha256:cdif",
                            "osi-hash": "sha256:osi",
                            "odrl-hash": "sha256:odrl",
                            "policy-bindings-hash": "sha256:policies"
                        }],
                        "view-artifacts": [{
                            "stable-id": "lakecat:view:local:default:active_customers",
                            "osi-hash": "sha256:view-osi"
                        }],
                        "standards": ["OpenLineage", "Grust catalog graph"]
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            }]),
            delivered: Mutex::default(),
        });
        let graph = Arc::new(RecordingGraph::default());
        let lineage = Arc::new(RecordingLineage::default());
        let app = app(
            LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
                .with_integrations(
                    default_sail_engine(),
                    AllowAllGovernanceEngine::new(),
                    graph.clone(),
                    lineage.clone(),
                ),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/management/v1/lineage/drain")
                    .header("x-lakecat-agent-did", "did:example:agent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["delivered"], serde_json::json!(1));
        assert_eq!(
            payload["event-types"],
            serde_json::json!(["querygraph.bootstrap"])
        );
        assert_eq!(payload["graph-events"], serde_json::json!(1));
        assert_eq!(payload["lineage-events"], serde_json::json!(1));
        assert_eq!(
            payload["events"][0]["event-id"],
            serde_json::json!("evt-bootstrap")
        );
        assert_eq!(
            payload["events"][0]["event-type"],
            serde_json::json!("querygraph.bootstrap")
        );
        assert_eq!(
            payload["events"][0]["principal-subject"],
            serde_json::json!("did:example:agent")
        );
        assert_eq!(
            payload["events"][0]["principal-kind"],
            serde_json::json!("agent")
        );
        assert!(
            payload["events"][0]["authorization-receipt-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert_eq!(
            payload["events"][0]["request-identity-state"],
            serde_json::json!("verified")
        );
        assert!(
            payload["events"][0]["agent-delegation-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert!(
            payload["events"][0]["agent-summary-signature-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert_eq!(payload["events"][0]["graph-events"], serde_json::json!(1));
        assert_eq!(payload["events"][0]["lineage-events"], serde_json::json!(1));
        assert_eq!(
            payload["events"][0]["bundle-hash"],
            serde_json::json!("sha256:bundle")
        );
        assert_eq!(
            payload["events"][0]["graph-hash"],
            serde_json::json!("sha256:graph")
        );
        assert_eq!(
            payload["events"][0]["open-lineage-hash"],
            serde_json::json!("sha256:openlineage")
        );
        assert_eq!(
            payload["events"][0]["querygraph-import-hash"],
            serde_json::json!("sha256:querygraph-import")
        );
        assert_eq!(
            payload["events"][0]["table-artifact-count"],
            serde_json::json!(1)
        );
        assert_eq!(
            payload["events"][0]["view-artifact-count"],
            serde_json::json!(1)
        );
        assert_eq!(
            payload["events"][0]["policy-binding-count"],
            serde_json::json!(1)
        );
        assert_eq!(
            payload["events"][0]["standards"],
            serde_json::json!(["OpenLineage", "Grust catalog graph"])
        );
        assert_eq!(
            payload["events"][0]["replay-event-hashes"],
            serde_json::json!(["recorded"])
        );
        assert_eq!(
            payload["events"][0]["replay-open-lineage-hashes"],
            serde_json::json!(["recorded-openlineage"])
        );
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &["evt-bootstrap".to_string()]
        );
        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 1);
        assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
        assert_eq!(
            graph_events[0].subject,
            "lakecat:principal:did:example:agent"
        );
        assert_eq!(
            graph_events[0].event_id.as_deref(),
            Some("evt-bootstrap:principal")
        );
        drop(graph_events);
        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 1);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::QueryGraphBootstrap
        );
        assert_eq!(lineage_events[0].principal.subject, "did:example:agent");
        assert_eq!(
            lineage_events[0].payload["bundle-hash"],
            serde_json::json!("sha256:bundle")
        );
        assert_eq!(
            lineage_events[0].payload["graph-hash"],
            serde_json::json!("sha256:graph")
        );
        assert_eq!(
            lineage_events[0].payload["querygraph-import-hash"],
            serde_json::json!("sha256:querygraph-import")
        );
        assert_eq!(
            lineage_events[0].payload["table-artifacts"][0]["croissant-hash"],
            serde_json::json!("sha256:croissant")
        );
        assert_eq!(
            lineage_events[0].payload["table-artifacts"][0]["policy-bindings-hash"],
            serde_json::json!("sha256:policies")
        );
        assert_eq!(
            lineage_events[0].payload["view-artifacts"][0]["osi-hash"],
            serde_json::json!("sha256:view-osi")
        );
        assert_eq!(
            lineage_events[0].payload["standards"],
            serde_json::json!(["OpenLineage", "Grust catalog graph"])
        );
    }

    #[tokio::test]
    async fn create_load_commit_and_plan_table_round_trips_through_integrations() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true,"doc":"Event identifier."}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
            .unwrap();
        let response = app.clone().oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let plan = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"select":["id"],"filter":{"type":"always-true"},"case-sensitive":true,"limit":10}"#))
            .unwrap();
        let response = app.clone().oneshot(plan).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["status"], serde_json::json!("completed"));
        let _: sail_catalog_iceberg::models::PlanTableScanRequest =
            serde_json::from_value(serde_json::json!({
                "select": ["id"],
                "filter": {"type": "always-true"},
                "case-sensitive": true
            }))
            .unwrap();
        assert_eq!(
            payload["residual-filter"]["lakecat:scan-request"]["case-sensitive"],
            serde_json::json!(true)
        );
        #[cfg(feature = "sail-local")]
        {
            assert_eq!(
                payload["lakecat-plan-tasks"][0]["task-type"],
                serde_json::json!("manifest-list")
            );
            assert_eq!(
                payload["residual-filter"]["filters-accepted-by-sail"][0]["expression-type"],
                serde_json::json!("always-true")
            );
            let plan_task = payload["plan-tasks"][0]
                .as_str()
                .expect("plan task token")
                .to_string();

            let fetch = Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces/default/tables/events/fetch-scan-tasks")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "plan-task": plan_task }).to_string(),
                ))
                .unwrap();
            let response = app.oneshot(fetch).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let _: sail_catalog_iceberg::models::FetchScanTasksResult =
                serde_json::from_value(payload.clone()).unwrap();
            assert_eq!(
                payload["residual-filter"]["lakecat:sail-target"],
                serde_json::json!("sail_iceberg::io::load_manifest_list")
            );
        }
    }

    #[tokio::test]
    async fn prefixed_catalog_routes_target_requested_warehouse() {
        let app = test_app();
        let upsert_project = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/projects/default")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "display-name": "Default Project"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert_project).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        for (warehouse, location, metadata_location, uuid) in [
            (
                "local",
                "file:///tmp/local-events",
                "file:///tmp/local-events/metadata/00000.json",
                "11111111-1111-1111-1111-111111111111",
            ),
            (
                "other",
                "file:///tmp/other-events",
                "file:///tmp/other-events/metadata/00000.json",
                "22222222-2222-2222-2222-222222222222",
            ),
        ] {
            let upsert_warehouse = Request::builder()
                .method(Method::PUT)
                .uri(format!("/management/v1/warehouses/{warehouse}"))
                .header("content-type", "application/json")
                .header("x-lakecat-principal", "operator@example.com")
                .body(Body::from(
                    serde_json::json!({
                        "project-id": "default",
                        "storage-root": location,
                    })
                    .to_string(),
                ))
                .unwrap();
            let response = app.clone().oneshot(upsert_warehouse).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let create = Request::builder()
                .method(Method::POST)
                .uri(format!("/catalog/v1/{warehouse}/namespaces/default/tables"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "events",
                        "location": location,
                        "metadata-location": metadata_location,
                        "metadata": {
                            "format-version": 3,
                            "table-uuid": uuid,
                            "location": location,
                            "last-sequence-number": 7,
                            "last-updated-ms": 1710000000000_i64,
                            "last-column-id": 1,
                            "schemas": [{
                                "type": "struct",
                                "schema-id": 1,
                                "fields": [{
                                    "id": 1,
                                    "name": "id",
                                    "type": "string",
                                    "required": true
                                }]
                            }],
                            "current-schema-id": 1,
                            "partition-specs": [{"spec-id": 0, "fields": []}],
                            "default-spec-id": 0
                        }
                    })
                    .to_string(),
                ))
                .unwrap();
            let response = app.clone().oneshot(create).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let default_load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(default_load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["metadata-location"],
            serde_json::json!("file:///tmp/local-events/metadata/00000.json")
        );
        assert_eq!(
            body["metadata"]["location"],
            serde_json::json!("file:///tmp/local-events")
        );

        let prefixed_load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/other/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(prefixed_load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["metadata-location"],
            serde_json::json!("file:///tmp/other-events/metadata/00000.json")
        );
        assert_eq!(
            body["metadata"]["location"],
            serde_json::json!("file:///tmp/other-events")
        );

        let missing_warehouse = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/missing/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(missing_warehouse).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn commit_can_advance_metadata_location_extension() {
        let app = test_app();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-commit-metadata-{unique}"));
        let table_dir = root.join("events");
        let metadata_dir = table_dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let table_location = url::Url::from_directory_path(&table_dir)
            .expect("table dir URL")
            .to_string();
        let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
            .unwrap()
            .to_string();
        let committed_metadata_location = url::Url::from_file_path(metadata_dir.join("00001.json"))
            .unwrap()
            .to_string();
        let new_metadata = serde_json::json!({
            "format-version": 3,
            "table-uuid": "11111111-1111-1111-1111-111111111111",
            "location": table_location,
            "last-sequence-number": 8,
            "last-updated-ms": 1710000000100_i64,
            "last-column-id": 1,
            "schemas": [{
                "type": "struct",
                "schema-id": 1,
                "fields": [{
                    "id": 1,
                    "name": "id",
                    "type": "string",
                    "required": true,
                    "doc": "Event identifier."
                }]
            }],
            "current-schema-id": 1,
            "partition-specs": [{"spec-id": 0, "fields": []}],
            "default-spec-id": 0,
            "current-snapshot-id": 43,
            "snapshots": [{
                "snapshot-id": 43,
                "sequence-number": 8,
                "timestamp-ms": 1710000000100_i64,
                "summary": {"operation": "append"},
                "schema-id": 1
            }],
            "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
        });
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "name": "events",
                    "location": table_location,
                    "metadata-location": initial_metadata_location,
                    "metadata": {
                        "format-version": 3,
                        "table-uuid": "11111111-1111-1111-1111-111111111111",
                        "location": table_location,
                        "last-sequence-number": 7,
                        "last-updated-ms": 1710000000000_i64,
                        "last-column-id": 1,
                        "schemas": [{
                            "type": "struct",
                            "schema-id": 1,
                            "fields": [{
                                "id": 1,
                                "name": "id",
                                "type": "string",
                                "required": true,
                                "doc": "Event identifier."
                            }]
                        }],
                        "current-schema-id": 1,
                        "partition-specs": [{"spec-id": 0, "fields": []}],
                        "default-spec-id": 0,
                        "current-snapshot-id": 42,
                        "snapshots": [{
                            "snapshot-id": 42,
                            "sequence-number": 7,
                            "timestamp-ms": 1710000000000_i64,
                            "summary": {"operation": "append"},
                            "schema-id": 1
                        }],
                        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "requirements": [],
                    "updates": [],
                    "metadata-location": committed_metadata_location,
                    "metadata": new_metadata,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload["metadata-location"],
            serde_json::json!(committed_metadata_location)
        );
        let written_metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(metadata_dir.join("00001.json")).unwrap())
                .unwrap();
        assert_eq!(
            written_metadata["current-snapshot-id"],
            serde_json::json!(43)
        );

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload["metadata-location"],
            serde_json::json!(committed_metadata_location)
        );
        assert_eq!(
            payload["metadata"]["current-snapshot-id"],
            serde_json::json!(43)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn commit_replays_rest_idempotency_key() {
        let store = MemoryCatalogStore::new();
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            store.clone(),
        ));
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        for _ in 0..2 {
            let commit = Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces/default/tables/events/commit")
                .header("content-type", "application/json")
                .header("x-lakecat-idempotency-key", "commit:events:0001")
                .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
                .unwrap();
            let response = app.clone().oneshot(commit).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                payload["metadata-location"],
                serde_json::json!("file:///tmp/events/metadata/00000.json")
            );
        }

        let mismatched_commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .header("x-lakecat-idempotency-key", "commit:events:0001")
            .body(Body::from(
                r#"{"requirements":[],"updates":[],"metadata-location":"file:///tmp/events/metadata/00001.json"}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(mismatched_commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
        let records = store.table_commit_records(&ident, 0, None).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].sequence_number, 1);
        assert_eq!(
            records[0].idempotency_key_sha256.as_deref(),
            Some(content_hash_bytes("commit:events:0001".as_bytes()).as_str())
        );
        assert_eq!(store.load_table(&ident).await.unwrap().version, 1);
    }

    #[cfg(feature = "sail-local")]
    #[tokio::test]
    async fn fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens() {
        let fixture = local_manifest_fixture();
        let app = test_app();
        let upsert_policy = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/policies/agent-id-read")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "namespace": ["default"],
                    "table": "events",
                    "enforced": true,
                    "odrl": {
                        "uid": "policy:agent-id-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["id"],
                            "row-predicate": {"type": "always-true"}
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert_policy).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let create_payload = serde_json::json!({
            "name": "events",
            "location": fixture.table_location,
            "metadata-location": fixture.metadata_location,
            "metadata": fixture.metadata,
        });
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(create_payload.to_string()))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let plan = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "select": ["id"],
                    "filter": {"type": "always-true"},
                    "case-sensitive": true,
                    "stats-fields": ["id"]
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(plan).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let _: sail_catalog_iceberg::models::CompletedPlanningWithIdResult =
            serde_json::from_value(payload.clone()).unwrap();
        assert_eq!(payload["status"], serde_json::json!("completed"));
        assert_eq!(
            payload["residual-filter"]["lakecat:scan-request"]["stats-fields"][0],
            serde_json::json!("id")
        );
        assert_eq!(
            payload["residual-filter"]["lakecat:scan-request"]["read-restriction"],
            serde_json::json!({
                "allowed-columns": ["id"],
                "row-predicate": {"type": "always-true"},
                "policy-hashes": [
                    lakecat_core::content_hash_json(&serde_json::json!({
                        "uid": "policy:agent-id-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["id"],
                            "row-predicate": {"type": "always-true"}
                        }
                    })).unwrap()
                ]
            })
        );
        assert_eq!(
            payload["residual-filter"]["filters-accepted-by-sail"][0]["filter"],
            serde_json::json!({"type": "always-true"})
        );
        let plan_task = payload["plan-tasks"][0]
            .as_str()
            .expect("plan task token")
            .to_string();

        let fetch = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/tasks")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "plan-task": plan_task }).to_string(),
            ))
            .unwrap();
        let response = app.oneshot(fetch).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let _: sail_catalog_iceberg::models::FileScanTask =
            serde_json::from_value(payload["file-scan-tasks"][0].clone()).unwrap();
        let _: sail_catalog_iceberg::models::PositionDeleteFile =
            serde_json::from_value(payload["delete-files"][0].clone()).unwrap();

        assert!(payload["plan-tasks"][0].as_str().is_some());
        assert_eq!(
            payload["lakecat-plan-tasks"][0]["task-type"],
            serde_json::json!("manifest")
        );
        assert_eq!(
            payload["file-scan-tasks"][0]["delete-file-references"][0],
            serde_json::json!(0)
        );
        assert_eq!(
            payload["delete-files"][0]["file-path"],
            serde_json::json!(fixture.delete_file_path)
        );
        assert_eq!(
            payload["residual-filter"]["lakecat:fetch-scan-tasks"]["read-restriction"],
            serde_json::json!({
                "allowed-columns": ["id"],
                "row-predicate": {"type": "always-true"},
                "policy-hashes": [
                    lakecat_core::content_hash_json(&serde_json::json!({
                        "uid": "policy:agent-id-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["id"],
                            "row-predicate": {"type": "always-true"}
                        }
                    })).unwrap()
                ]
            })
        );

        let _ = std::fs::remove_dir_all(fixture.root);
    }

    #[tokio::test]
    async fn plan_rejects_invalid_incremental_scan_modes() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true,"doc":"Event identifier."}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let mixed = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "snapshot-id": 42,
                    "start-snapshot-id": 1,
                    "end-snapshot-id": 42
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(mixed).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let missing_end = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({"start-snapshot-id": 1}).to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(missing_end).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let missing_start = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({"end-snapshot-id": 42}).to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(missing_start).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        #[cfg(feature = "sail-local")]
        {
            let invalid_range = Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces/default/tables/events/plan")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "start-snapshot-id": 1,
                        "end-snapshot-id": 42
                    })
                    .to_string(),
                ))
                .unwrap();
            let response = app.clone().oneshot(invalid_range).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }

        let valid_empty_delta = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "start-snapshot-id": 42,
                    "end-snapshot-id": 42
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(valid_empty_delta).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["snapshot-id"], serde_json::json!(42));
        assert_eq!(body["plan-tasks"], serde_json::json!([]));
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["start-snapshot-id"],
            serde_json::json!(42)
        );
    }

    #[tokio::test]
    async fn querygraph_bootstrap_projects_catalog_tables() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true,"doc":"Event identifier.","semantic-type":"https://schema.org/identifier"}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let policy = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/policies/agent-read")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "namespace": ["default"],
                    "table": "events",
                    "enforced": true,
                    "odrl": {
                        "uid": "policy:agent-read",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"]
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(policy).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bootstrap = Request::builder()
            .method(Method::GET)
            .uri("/querygraph/v1/bootstrap")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(bootstrap).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            body["bundle-hash"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert!(
            body["manifest"]["graph-hash"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert!(
            body["manifest"]["open-lineage-hash"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert_eq!(
            body["manifest"]["querygraph-import"]["schema-version"],
            serde_json::json!("lakecat.querygraph.import-compat.v1")
        );
        assert!(
            body["manifest"]["querygraph-import"]["table-only-bundle-hash"]
                .as_str()
                .is_some_and(|value| value.starts_with("sha256:"))
        );
        assert_eq!(
            body["manifest"]["querygraph-import"]["graph-hash"],
            body["manifest"]["graph-hash"]
        );
        assert_eq!(
            body["manifest"]["querygraph-import"]["view-count"],
            serde_json::json!(0)
        );
        assert_eq!(
            body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["graphHash"],
            body["manifest"]["graph-hash"]
        );
        assert_eq!(
            body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]
                ["stableId"],
            body["manifest"]["table-artifacts"][0]["stable-id"]
        );
        assert_eq!(
            body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]
                ["croissantHash"],
            body["manifest"]["table-artifacts"][0]["croissant-hash"]
        );
        assert_eq!(
            body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["tableArtifacts"][0]
                ["policyBindingsHash"],
            body["manifest"]["table-artifacts"][0]["policy-bindings-hash"]
        );
        assert!(
            body["manifest"]["standards"]
                .as_array()
                .unwrap()
                .iter()
                .any(|standard| standard == "Grust catalog graph")
        );
        assert_eq!(
            body["tables"][0]["policy-bindings"][0]["policy-id"],
            "agent-read"
        );
        assert_eq!(
            body["tables"][0]["policy-bindings"][0]["odrl"]["lakecat:read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["tables"][0]["odrl"]["lakecat:policy-bindings"][0]["odrl"]["lakecat:read-restriction"]
                ["allowed-columns"],
            serde_json::json!(["event_id"])
        );
    }

    #[tokio::test]
    async fn querygraph_bootstrap_projects_catalog_views() {
        let app = test_app();
        let namespace = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"namespace":["default"]}"#))
            .unwrap();
        let response = app.clone().oneshot(namespace).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let view = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "sql": "select id, email from customers where active",
                    "dialect": "sql",
                    "schema-version": 1,
                    "columns": [
                        {
                            "name": "id",
                            "data-type": "int",
                            "nullable": false,
                            "comment": "Customer identifier"
                        },
                        {
                            "name": "email",
                            "data-type": "string",
                            "nullable": true
                        }
                    ],
                    "properties": {
                        "semantic-domain": "customer"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(view).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bootstrap = Request::builder()
            .method(Method::GET)
            .uri("/querygraph/v1/bootstrap")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(bootstrap).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["views"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["views"][0]["name"],
            serde_json::json!("active_customers")
        );
        assert_eq!(
            body["manifest"]["view-artifacts"].as_array().unwrap().len(),
            1
        );
        assert!(
            body["graph"]["edges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|edge| edge["label"] == serde_json::json!("CONTAINS_VIEW"))
        );
        assert_eq!(
            body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["viewCount"],
            serde_json::json!(1)
        );
        assert_eq!(
            body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["viewArtifacts"][0]
                ["stableId"],
            body["manifest"]["view-artifacts"][0]["stable-id"]
        );
        assert_eq!(
            body["open-lineage"]["run"]["facets"]["queryGraph_semanticBundle"]["viewArtifacts"][0]
                ["osiHash"],
            body["manifest"]["view-artifacts"][0]["osi-hash"]
        );
    }

    #[tokio::test]
    async fn load_credentials_returns_scoped_local_file_profile_without_raw_secrets() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let credentials = body["storage-credentials"].as_array().unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(
            credentials[0]["prefix"],
            serde_json::json!("file:///tmp/events")
        );
        let config = credentials[0]["config"].as_array().unwrap();
        assert!(config.iter().any(|entry| {
            entry["key"] == "lakecat.credential-mode" && entry["value"] == "local-file-no-secret"
        }));
        assert!(!config.iter().any(|entry| {
            entry["key"]
                .as_str()
                .is_some_and(|key| key.contains("secret") || key.contains("token"))
        }));
    }

    #[tokio::test]
    async fn load_credentials_returns_empty_for_remote_profile_until_issuance_exists() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"s3://lakecat-demo/events","metadata-location":"s3://lakecat-demo/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["storage-credentials"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn delete_table_soft_deletes_from_catalog_reads() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let delete = Request::builder()
            .method(Method::DELETE)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(delete).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let delete_again = Request::builder()
            .method(Method::DELETE)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(delete_again).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn restore_table_reopens_soft_deleted_catalog_reads() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let delete = Request::builder()
            .method(Method::DELETE)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(delete).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let restore = Request::builder()
            .method(Method::POST)
            .uri("/management/v1/warehouses/local/namespaces/default/tables/events/restore")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(restore).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["identifier"]["name"], serde_json::json!("events"));

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn management_servers_are_durable_management_entities() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/servers/lakecat-local")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "display-name": "Local LakeCat",
                    "endpoint-url": "http://127.0.0.1:8181",
                    "properties": {
                        "deployment": "local"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["server-id"], serde_json::json!("lakecat-local"));
        assert_eq!(body["display-name"], serde_json::json!("Local LakeCat"));
        assert_eq!(
            body["endpoint-url"],
            serde_json::json!("http://127.0.0.1:8181")
        );
        assert_eq!(body["properties"]["deployment"], serde_json::json!("local"));

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/servers")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["servers"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["servers"][0]["server-id"],
            serde_json::json!("lakecat-local")
        );
    }

    #[tokio::test]
    async fn management_warehouses_are_durable_management_entities() {
        let app = test_app();
        let upsert_project = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/projects/default")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "display-name": "Default Project"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert_project).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let upsert_scoped = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/projects/default/warehouses/project_local")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "storage-root": "file:///tmp/lakecat-project-local",
                    "properties": {
                        "region": "project-scoped"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert_scoped).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["warehouse"], serde_json::json!("project_local"));
        assert_eq!(body["project-id"], serde_json::json!("default"));

        let scoped_list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/projects/default/warehouses")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(scoped_list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["warehouses"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["warehouses"][0]["warehouse"],
            serde_json::json!("project_local")
        );

        let mismatched_project = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/projects/default/warehouses/mismatch")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "project-id": "other",
                    "storage-root": "file:///tmp/mismatch"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(mismatched_project).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "project-id": "default",
                    "storage-root": "file:///tmp/lakecat",
                    "properties": {
                        "region": "local"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["warehouse"], serde_json::json!("local"));
        assert_eq!(body["project-id"], serde_json::json!("default"));
        assert_eq!(
            body["storage-root"],
            serde_json::json!("file:///tmp/lakecat")
        );
        assert_eq!(body["properties"]["region"], serde_json::json!("local"));

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["warehouses"].as_array().unwrap().len(), 2);
        assert!(
            body["warehouses"]
                .as_array()
                .unwrap()
                .iter()
                .any(|warehouse| { warehouse["warehouse"] == serde_json::json!("local") })
        );

        let other_warehouse = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/other")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "project-id": "default",
                    "storage-root": "file:///tmp/lakecat-other",
                    "properties": {
                        "region": "other"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(other_warehouse).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["warehouse"], serde_json::json!("other"));
        assert_eq!(
            body["storage-root"],
            serde_json::json!("file:///tmp/lakecat-other")
        );

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["warehouses"].as_array().unwrap().len(), 3);
        assert!(
            body["warehouses"]
                .as_array()
                .unwrap()
                .iter()
                .any(|warehouse| { warehouse["warehouse"] == serde_json::json!("other") })
        );

        let missing_project = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/orphaned")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "project-id": "missing-project",
                    "storage-root": "file:///tmp/orphaned"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(missing_project).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn management_projects_are_durable_management_entities() {
        let app = test_app();
        let upsert_server = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/servers/lakecat-local")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "display-name": "Local LakeCat"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert_server).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/projects/default")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "server-id": "lakecat-local",
                    "display-name": "Default Project",
                    "properties": {
                        "owner": "querygraph"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["project-id"], serde_json::json!("default"));
        assert_eq!(body["server-id"], serde_json::json!("lakecat-local"));
        assert_eq!(body["display-name"], serde_json::json!("Default Project"));
        assert_eq!(body["properties"]["owner"], serde_json::json!("querygraph"));

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/projects")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["projects"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["projects"][0]["project-id"],
            serde_json::json!("default")
        );
        assert_eq!(
            body["projects"][0]["server-id"],
            serde_json::json!("lakecat-local")
        );
        assert_eq!(
            body["projects"][0]["display-name"],
            serde_json::json!("Default Project")
        );

        let missing_server = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/projects/orphaned")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "server-id": "missing-server",
                    "display-name": "Orphaned Project"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(missing_server).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn management_views_are_durable_management_entities() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "sql": "select id, email from customers where active",
                    "dialect": "sql",
                    "schema-version": 1,
                    "columns": [
                        {
                            "name": "id",
                            "data-type": "int",
                            "nullable": false,
                            "comment": "Customer identifier"
                        },
                        {
                            "name": "email",
                            "data-type": "string",
                            "nullable": true
                        }
                    ],
                    "properties": {
                        "semantic-domain": "customer"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["warehouse"], serde_json::json!("local"));
        assert_eq!(body["namespace"], serde_json::json!(["default"]));
        assert_eq!(body["name"], serde_json::json!("active_customers"));
        assert_eq!(
            body["properties"]["semantic-domain"],
            serde_json::json!("customer")
        );
        assert_eq!(body["columns"].as_array().unwrap().len(), 2);
        assert_eq!(body["columns"][0]["name"], serde_json::json!("id"));
        assert_eq!(body["columns"][0]["data-type"], serde_json::json!("int"));
        assert_eq!(body["columns"][0]["nullable"], serde_json::json!(false));

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses/local/namespaces/default/views")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["views"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["views"][0]["name"],
            serde_json::json!("active_customers")
        );
        assert_eq!(body["views"][0]["schema-version"], serde_json::json!(1));
        assert_eq!(
            body["views"][0]["columns"][0]["comment"],
            serde_json::json!("Customer identifier")
        );

        let catalog_list = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/local/namespaces/default/views")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(catalog_list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["views"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["views"][0]["name"],
            serde_json::json!("active_customers")
        );

        let catalog_load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/local/namespaces/default/views/active_customers")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(catalog_load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["name"], serde_json::json!("active_customers"));
        assert_eq!(body["schema-version"], serde_json::json!(1));
        assert_eq!(
            body["properties"]["semantic-domain"],
            serde_json::json!("customer")
        );

        let catalog_upsert = Request::builder()
            .method(Method::PUT)
            .uri("/catalog/v1/local/namespaces/default/views/catalog_customers")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "sql": "select id from customers where active",
                    "dialect": "sql",
                    "schema-version": 2,
                    "properties": {
                        "semantic-domain": "catalog-customer"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(catalog_upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let catalog_load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/local/namespaces/default/views/catalog_customers")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(catalog_load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["name"], serde_json::json!("catalog_customers"));
        assert_eq!(body["schema-version"], serde_json::json!(2));

        let catalog_drop = Request::builder()
            .method(Method::DELETE)
            .uri("/catalog/v1/local/namespaces/default/views/catalog_customers")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(catalog_drop).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let dropped_catalog_load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/local/namespaces/default/views/catalog_customers")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(dropped_catalog_load).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let management_drop = Request::builder()
            .method(Method::DELETE)
            .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(management_drop).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses/local/namespaces/default/views")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["views"].as_array().unwrap().len(), 0);

        let repeated_drop = Request::builder()
            .method(Method::DELETE)
            .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(repeated_drop).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let missing = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/local/namespaces/default/views/missing_view")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(missing).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn management_storage_profile_overrides_inferred_credentials_by_prefix() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/local-events")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "file:///tmp/events",
                    "provider": "file",
                    "issuance-mode": "local-file-no-secret",
                    "public-config": {
                        "lakecat.endpoint": "local"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["profile-id"], serde_json::json!("local-events"));

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses/local/storage-profiles")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["storage-profiles"].as_array().unwrap().len(), 1);

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events/tenant-a","metadata-location":"file:///tmp/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let credentials = body["storage-credentials"].as_array().unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(
            credentials[0]["prefix"],
            serde_json::json!("file:///tmp/events")
        );
        let config = credentials[0]["config"].as_array().unwrap();
        assert!(config.iter().any(|entry| {
            entry["key"] == "lakecat.storage-profile-id" && entry["value"] == "local-events"
        }));
        assert!(
            config
                .iter()
                .any(|entry| { entry["key"] == "lakecat.endpoint" && entry["value"] == "local" })
        );
    }

    #[tokio::test]
    async fn remote_storage_profile_accepts_secret_ref_without_vending_raw_secrets() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/s3-events")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "s3://lakecat-demo/events",
                    "provider": "s3",
                    "issuance-mode": "short-lived-secret-ref",
                    "secret-ref": "typesec://lakecat/local/s3-events",
                    "public-config": {
                        "lakecat.region": "us-west-2"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["secret-ref"],
            serde_json::json!("typesec://lakecat/local/s3-events")
        );

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"s3://lakecat-demo/events/tenant-a","metadata-location":"s3://lakecat-demo/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["storage-credentials"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn credential_issuer_vends_short_lived_credentials_for_secret_ref_profile() {
        let issuer = Arc::new(RecordingCredentialIssuer::default());
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        )
        .with_credential_issuer(issuer.clone()));
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/s3-events")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "s3://lakecat-demo/events",
                    "provider": "s3",
                    "issuance-mode": "short-lived-secret-ref",
                    "secret-ref": "typesec://lakecat/local/s3-events"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"s3://lakecat-demo/events/tenant-a","metadata-location":"s3://lakecat-demo/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let credentials = body["storage-credentials"].as_array().unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(
            credentials[0]["prefix"],
            serde_json::json!("s3://lakecat-demo/events")
        );
        let config = credentials[0]["config"].as_array().unwrap();
        assert!(config.iter().any(|entry| {
            entry["key"] == "lakecat.credential-kind" && entry["value"] == "mock-short-lived"
        }));
        assert!(
            !config
                .iter()
                .any(|entry| { entry["value"] == "typesec://lakecat/local/s3-events" })
        );

        let requests = issuer.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].profile.secret_ref.as_deref(),
            Some("typesec://lakecat/local/s3-events")
        );
        assert_eq!(
            requests[0].authorization_receipt.principal.subject,
            "did:example:agent"
        );
    }

    #[cfg(feature = "typesec-local")]
    #[tokio::test]
    async fn typesec_credential_issuer_gates_secret_ref_resolution() {
        use crate::typesec_credential_issuer::{
            EnvironmentSecretRefCredentialResolver, TypeSecCredentialIssuer,
        };

        let issuer = TypeSecCredentialIssuer::new(
            Arc::new(AllowCredentialIssuePolicy {
                subject: "did:example:agent".to_string(),
                resource: "typesec://env/LAKECAT_S3_EVENTS_CREDENTIALS".to_string(),
            }),
            EnvironmentSecretRefCredentialResolver::with_reader(|name| {
                if name == "LAKECAT_S3_EVENTS_CREDENTIALS" {
                    Ok(serde_json::json!({
                        "lakecat.credential-kind": "typesec-env-short-lived",
                        "aws.session-token": "temporary-typesec-token"
                    })
                    .to_string())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            }),
        );
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        )
        .with_credential_issuer(issuer));
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/s3-events")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "s3://lakecat-demo/events",
                    "provider": "s3",
                    "issuance-mode": "short-lived-secret-ref",
                    "secret-ref": "typesec://env/LAKECAT_S3_EVENTS_CREDENTIALS"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"s3://lakecat-demo/events/tenant-a","metadata-location":"s3://lakecat-demo/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let credentials = body["storage-credentials"].as_array().unwrap();
        assert_eq!(credentials.len(), 1);
        let config = credentials[0]["config"].as_array().unwrap();
        assert!(config.iter().any(|entry| {
            entry["key"] == "lakecat.credential-kind" && entry["value"] == "typesec-env-short-lived"
        }));

        let denied = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:other")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(denied).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[cfg(feature = "typesec-local")]
    #[tokio::test]
    async fn typesec_credential_issuer_gates_production_secret_refs_before_dispatch() {
        use crate::typesec_credential_issuer::{
            ExternalSecretRefCredentialResolver, SecretRefProvider, TypeSecCredentialIssuer,
            secret_ref_provider,
        };

        let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
        let table = TableRecord::new(
            table_ident("local", "default", "events").unwrap(),
            "s3://lakecat-demo/events/tenant-a".to_string(),
            Some("s3://lakecat-demo/events/tenant-a/metadata/00000.json".to_string()),
            serde_json::json!({"format-version":3}),
            principal.clone(),
        );
        let profile = StorageProfile::new(
            "s3-events",
            WarehouseName::new("local").unwrap(),
            "s3://lakecat-demo/events",
            StorageProvider::S3,
            CredentialIssuanceMode::ShortLivedSecretRef,
            Some("vault://secret/data/lakecat/s3-events".to_string()),
            Default::default(),
        )
        .unwrap();
        let request = CredentialIssuanceRequest {
            table,
            profile,
            authorization_receipt: AuthorizationReceipt {
                principal,
                action: CatalogAction::CredentialsVend,
                table: Some(table_ident("local", "default", "events").unwrap()),
                allowed: true,
                engine: "test".to_string(),
                policy_hash: None,
                context: serde_json::json!({}),
                checked_at: chrono::Utc::now(),
            },
        };

        for (secret_ref, provider_label) in [
            (
                "vault://secret/data/lakecat/s3-events",
                SecretRefProvider::Vault.as_str(),
            ),
            (
                "aws-sm://lakecat/s3-events",
                SecretRefProvider::AwsSecretsManager.as_str(),
            ),
            (
                "gcp-sm://lakecat/s3-events",
                SecretRefProvider::GcpSecretManager.as_str(),
            ),
            (
                "azure-kv://lakecat/s3-events",
                SecretRefProvider::AzureKeyVault.as_str(),
            ),
        ] {
            let mut request = request.clone();
            request.profile.secret_ref = Some(secret_ref.to_string());
            assert_eq!(
                secret_ref_provider(secret_ref).unwrap().as_str(),
                provider_label
            );

            let issuer = TypeSecCredentialIssuer::new(
                Arc::new(AllowCredentialIssuePolicy {
                    subject: "did:example:agent".to_string(),
                    resource: secret_ref.to_string(),
                }),
                ExternalSecretRefCredentialResolver::with_env_reader(|_| {
                    Err(std::env::VarError::NotPresent)
                }),
            );
            let err = issuer.issue(request.clone()).await.unwrap_err();
            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            assert!(err.to_string().contains(&format!(
                "credential secret resolver for {provider_label} is not configured"
            )));

            let denied = TypeSecCredentialIssuer::new(
                Arc::new(AllowCredentialIssuePolicy {
                    subject: "did:example:other".to_string(),
                    resource: secret_ref.to_string(),
                }),
                ExternalSecretRefCredentialResolver::with_env_reader(|_| {
                    Err(std::env::VarError::NotPresent)
                }),
            );
            let err = denied.issue(request).await.unwrap_err();
            assert!(matches!(err, LakeCatError::Conflict(_)));
            assert!(
                err.to_string()
                    .contains("TypeSec denied credential issuance")
            );
        }
    }

    #[cfg(feature = "typesec-local")]
    #[tokio::test]
    async fn typesec_credential_issuer_resolves_vault_secret_refs_after_authorization() {
        use crate::typesec_credential_issuer::{
            ExternalSecretRefCredentialResolver, TypeSecCredentialIssuer,
            VaultSecretRefCredentialResolver,
        };

        let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
        let table = TableRecord::new(
            table_ident("local", "default", "events").unwrap(),
            "s3://lakecat-demo/events/tenant-a".to_string(),
            Some("s3://lakecat-demo/events/tenant-a/metadata/00000.json".to_string()),
            serde_json::json!({"format-version":3}),
            principal.clone(),
        );
        let profile = StorageProfile::new(
            "s3-events",
            WarehouseName::new("local").unwrap(),
            "s3://lakecat-demo/events",
            StorageProvider::S3,
            CredentialIssuanceMode::ShortLivedSecretRef,
            Some("vault://secret/data/lakecat/s3-events".to_string()),
            Default::default(),
        )
        .unwrap();
        let request = CredentialIssuanceRequest {
            table,
            profile,
            authorization_receipt: AuthorizationReceipt {
                principal,
                action: CatalogAction::CredentialsVend,
                table: Some(table_ident("local", "default", "events").unwrap()),
                allowed: true,
                engine: "test".to_string(),
                policy_hash: None,
                context: serde_json::json!({}),
                checked_at: chrono::Utc::now(),
            },
        };
        let vault_client = Arc::new(MockVaultSecretClient::default());
        *vault_client.response.lock().await = Some(serde_json::json!({
            "data": {
                "data": {
                    "lakecat.credential-kind": "vault-short-lived",
                    "aws.session-token": "temporary-vault-token"
                },
                "metadata": {
                    "version": 7
                }
            }
        }));
        let vault = VaultSecretRefCredentialResolver::new(
            "https://vault.example.test/",
            "vault-token",
            Some("lakecat/admin".to_string()),
            vault_client.clone(),
        )
        .unwrap();
        let issuer = TypeSecCredentialIssuer::new(
            Arc::new(AllowCredentialIssuePolicy {
                subject: "did:example:agent".to_string(),
                resource: "vault://secret/data/lakecat/s3-events".to_string(),
            }),
            ExternalSecretRefCredentialResolver::with_vault(vault),
        );

        let credentials = issuer.issue(request).await.unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].prefix, "s3://lakecat-demo/events");
        assert!(credentials[0].config.iter().any(|entry| {
            entry.key == "lakecat.credential-kind" && entry.value == "vault-short-lived"
        }));
        assert!(credentials[0].config.iter().any(|entry| {
            entry.key == "aws.session-token" && entry.value == "temporary-vault-token"
        }));

        let requests = vault_client.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].0,
            "https://vault.example.test/v1/secret/data/lakecat/s3-events"
        );
        assert_eq!(requests[0].1, "vault-token");
        assert_eq!(requests[0].2.as_deref(), Some("lakecat/admin"));
    }

    #[cfg(feature = "typesec-local")]
    #[test]
    fn environment_secret_resolver_parses_supported_secret_shapes() {
        use crate::typesec_credential_issuer::{
            SecretRefProvider, config_entries_from_secret_json,
            config_entries_from_vault_secret_json, env_secret_variable, secret_ref_provider,
            vault_secret_path,
        };

        assert_eq!(
            env_secret_variable("typesec://env/LAKECAT_S3_EVENTS").unwrap(),
            "LAKECAT_S3_EVENTS"
        );
        assert!(env_secret_variable("typesec://env/lowercase").is_err());
        assert!(env_secret_variable("typesec://vault/path").is_err());
        assert_eq!(
            secret_ref_provider("typesec://env/LAKECAT_S3_EVENTS").unwrap(),
            SecretRefProvider::TypeSecEnv
        );
        assert_eq!(
            secret_ref_provider("vault://secret/data/lakecat/s3-events").unwrap(),
            SecretRefProvider::Vault
        );
        assert_eq!(
            vault_secret_path("vault://secret/data/lakecat/s3-events").unwrap(),
            "v1/secret/data/lakecat/s3-events"
        );
        assert_eq!(
            secret_ref_provider("aws-sm://lakecat/s3-events").unwrap(),
            SecretRefProvider::AwsSecretsManager
        );
        assert_eq!(
            secret_ref_provider("gcp-sm://lakecat/s3-events").unwrap(),
            SecretRefProvider::GcpSecretManager
        );
        assert_eq!(
            secret_ref_provider("azure-kv://lakecat/s3-events").unwrap(),
            SecretRefProvider::AzureKeyVault
        );
        assert!(secret_ref_provider("file:///tmp/raw-secret").is_err());

        let object_entries = config_entries_from_secret_json(
            r#"{"aws.session-token":"temporary-token","aws.region":"us-west-2"}"#,
        )
        .unwrap();
        assert!(
            object_entries
                .iter()
                .any(|entry| entry.key == "aws.session-token" && entry.value == "temporary-token")
        );

        let array_entries = config_entries_from_secret_json(
            r#"[{"key":"lakecat.credential-kind","value":"typesec-env-short-lived"}]"#,
        )
        .unwrap();
        assert_eq!(
            array_entries,
            vec![ConfigEntry::new(
                "lakecat.credential-kind",
                "typesec-env-short-lived"
            )]
        );

        assert!(config_entries_from_secret_json(r#"{"aws.session-token":123}"#).is_err());

        let vault_entries = config_entries_from_vault_secret_json(serde_json::json!({
            "data": {
                "data": {
                    "aws.session-token": "temporary-token",
                    "aws.region": "us-west-2"
                }
            }
        }))
        .unwrap();
        assert!(
            vault_entries
                .iter()
                .any(|entry| entry.key == "aws.session-token" && entry.value == "temporary-token")
        );
        assert!(
            config_entries_from_vault_secret_json(serde_json::json!({
                "data": {
                    "data": {
                        "aws.session-token": 123
                    }
                }
            }))
            .is_err()
        );
    }

    #[tokio::test]
    async fn policy_bindings_are_governed_and_attached_to_table_authorization_context() {
        let governance = Arc::new(RecordingGovernance::default());
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        )
        .with_integrations(
            default_sail_engine(),
            governance.clone(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ));

        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/policies/agent-read")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "namespace": ["default"],
                    "table": "events",
                    "enforced": true,
                    "odrl": {
                        "uid": "policy:agent-read",
                        "permission": [{
                            "action": "read",
                            "constraint": [{
                                "leftOperand": "purpose",
                                "operator": "eq",
                                "rightOperand": "resilience-demo"
                            }]
                        }]
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses/local/policies")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["policies"].as_array().unwrap().len(), 1);

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let contexts = governance.contexts.lock().await;
        let load_context = contexts
            .iter()
            .find(|context| {
                context["policy-bindings"]
                    .as_array()
                    .is_some_and(|bindings| !bindings.is_empty())
            })
            .expect("table authorization should include active policy bindings");
        assert_eq!(
            load_context["policy-bindings"][0]["policy-id"],
            serde_json::json!("agent-read")
        );
        assert_eq!(
            load_context["policy-bindings"][0]["odrl"]["uid"],
            serde_json::json!("policy:agent-read")
        );
    }

    #[tokio::test]
    async fn table_scan_authorization_carries_policy_read_restriction() {
        let store = MemoryCatalogStore::new();
        let governance = Arc::new(RecordingGovernance::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_integrations(
                default_sail_engine(),
                governance.clone(),
                NoopCatalogGraphSink::new(),
                HashOnlyLineageSink::new(),
            );
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            Namespace::new(vec!["default".to_string()]).unwrap(),
            TableName::new("events").unwrap(),
        );
        store
            .upsert_policy_binding(
                PolicyBinding::new(
                    "agent-columns",
                    WarehouseName::new("local").unwrap(),
                    Some(ident.namespace.clone()),
                    Some(ident.name.clone()),
                    true,
                    serde_json::json!({
                        "uid": "policy:agent-columns",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"],
                            "row-predicate": {
                                "type": "eq",
                                "term": "event_id",
                                "value": "evt-1"
                            }
                        },
                        "permission": [{
                            "action": "read",
                            "constraint": [{
                                "leftOperand": "purpose",
                                "operator": "eq",
                                "rightOperand": "resilience-demo"
                            }]
                        }]
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let capability = authorize_table_scan(
            &state,
            RequestIdentity {
                principal: Principal::new("did:example:agent", PrincipalKind::Agent).unwrap(),
                envelope: serde_json::json!({"type": "test"}),
                typedid_envelope: None,
            },
            ident,
        )
        .await
        .unwrap();
        let restriction = capability.read_restriction().unwrap();
        assert_eq!(
            restriction.allowed_columns.as_deref(),
            Some(&["event_id".to_string()][..])
        );
        assert_eq!(restriction.purpose.as_deref(), Some("resilience-demo"));
        assert_eq!(
            restriction.row_predicate,
            Some(serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            }))
        );
        assert_eq!(restriction.policy_hashes.len(), 1);
        assert!(
            capability.receipt().policy_hash.is_some(),
            "governed scan receipt should summarize enforced policy hashes"
        );

        let contexts = governance.contexts.lock().await;
        assert_eq!(
            contexts[0]["read-restriction"]["allowed-columns"][0],
            serde_json::json!("event_id")
        );
        assert_eq!(
            contexts[0]["read-restriction"]["row-predicate"],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );
    }

    #[tokio::test]
    async fn credential_vend_blocks_raw_credentials_for_fine_grained_restriction() {
        let store = MemoryCatalogStore::new();
        let issuer = Arc::new(RecordingCredentialIssuer::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_credential_issuer(issuer.clone());
        let create = TableRecord::new(
            TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            ),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({
                "format-version": 3,
                "current-schema-id": 1,
                "schemas": [{
                    "schema-id": 1,
                    "fields": [
                        {"id": 1, "name": "event_id", "type": "string", "required": true},
                        {"id": 2, "name": "payload", "type": "string", "required": false}
                    ]
                }]
            }),
            Principal::anonymous(),
        );
        let ident = create.ident.clone();
        store.create_table(create).await.unwrap();
        store
            .upsert_policy_binding(
                PolicyBinding::new(
                    "agent-credential-columns",
                    WarehouseName::new("local").unwrap(),
                    Some(ident.namespace.clone()),
                    Some(ident.name.clone()),
                    true,
                    serde_json::json!({
                        "uid": "policy:agent-credential-columns",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"],
                            "row-predicate": {
                                "type": "eq",
                                "term": "event_id",
                                "value": "evt-1"
                            },
                            "max-credential-ttl-seconds": 300
                        }
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-lakecat-agent-did",
            axum::http::HeaderValue::from_static("did:example:agent"),
        );
        let response = load_credentials(
            State(state),
            headers,
            Path(("default".to_string(), "events".to_string())),
        )
        .await
        .unwrap();
        assert_eq!(response.0.storage_credentials.len(), 0);

        let requests = issuer.requests.lock().await;
        assert!(requests.is_empty());
        drop(requests);

        let outbox = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        let event = outbox
            .iter()
            .find(|event| event.event_type == "credentials.vend-attempted")
            .expect("credentials vend audit event");
        let receipt = &event.payload["payload"]["authorization-receipt"];
        assert!(
            receipt["policy_hash"].as_str().is_some(),
            "governed credential receipt should summarize enforced policy hashes"
        );
        assert_eq!(
            receipt["action"],
            serde_json::json!(CatalogAction::CredentialsVend)
        );
        assert_eq!(
            receipt["context"]["lakecat:raw-credential-exception"]["allowed"],
            serde_json::json!(false)
        );
        assert_eq!(
            receipt["context"]["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            receipt["context"]["read-restriction"]["row-predicate"],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );
        assert_eq!(
            receipt["context"]["read-restriction"]["max-credential-ttl-seconds"],
            serde_json::json!(300)
        );
        assert_eq!(
            event.payload["payload"]["credential-count"],
            serde_json::json!(0)
        );
        assert_eq!(
            event.payload["payload"]["lakecat:credential-block-reason"],
            serde_json::json!("fine-grained read restriction requires Sail-planned reads")
        );
    }

    #[tokio::test]
    async fn credential_vend_allows_trusted_human_raw_exception_for_restricted_table() {
        let store = MemoryCatalogStore::new();
        let issuer = Arc::new(RecordingCredentialIssuer::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_credential_issuer(issuer.clone());
        let create = TableRecord::new(
            TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            ),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({
                "format-version": 3,
                "current-schema-id": 1,
                "schemas": [{
                    "schema-id": 1,
                    "fields": [
                        {"id": 1, "name": "event_id", "type": "string", "required": true},
                        {"id": 2, "name": "payload", "type": "string", "required": false}
                    ]
                }]
            }),
            Principal::anonymous(),
        );
        let ident = create.ident.clone();
        store.create_table(create).await.unwrap();
        store
            .upsert_policy_binding(
                PolicyBinding::new(
                    "agent-credential-columns",
                    WarehouseName::new("local").unwrap(),
                    Some(ident.namespace.clone()),
                    Some(ident.name.clone()),
                    true,
                    serde_json::json!({
                        "uid": "policy:agent-credential-columns",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"],
                            "row-predicate": {
                                "type": "eq",
                                "term": "event_id",
                                "value": "evt-1"
                            },
                            "max-credential-ttl-seconds": 300
                        }
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-lakecat-principal",
            axum::http::HeaderValue::from_static("human:operator"),
        );
        let response = load_credentials(
            State(state),
            headers,
            Path(("default".to_string(), "events".to_string())),
        )
        .await
        .unwrap();
        assert_eq!(response.0.storage_credentials.len(), 1);
        assert_eq!(
            response.0.storage_credentials[0].prefix,
            "file:///tmp/events"
        );

        let requests = issuer.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].authorization_receipt.principal.kind,
            PrincipalKind::Human
        );
        drop(requests);

        let outbox = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        let event = outbox
            .iter()
            .find(|event| event.event_type == "credentials.vend-attempted")
            .expect("credentials vend audit event");
        let receipt = &event.payload["payload"]["authorization-receipt"];
        assert_eq!(
            receipt["context"]["lakecat:raw-credential-exception"]["allowed"],
            serde_json::json!(true)
        );
        assert_eq!(
            receipt["context"]["lakecat:raw-credential-exception"]["reason"],
            serde_json::json!("trusted human principal may use audited raw credential vending")
        );
        assert_eq!(
            event.payload["payload"]["credential-count"],
            serde_json::json!(1)
        );
        assert!(
            event.payload["payload"]
                .get("lakecat:credential-block-reason")
                .is_none()
        );
    }

    #[test]
    fn credentials_vend_audit_payload_surfaces_policy_context() {
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let table = TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({ "format-version": 3 }),
            Principal::anonymous(),
        );
        let profile = StorageProfile::inferred_for_table(&table);
        let receipt = AuthorizationReceipt {
            principal: Principal::new("did:example:agent", PrincipalKind::Agent).unwrap(),
            action: CatalogAction::CredentialsVend,
            table: Some(ident.clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: Some("policy-hash".to_string()),
            context: serde_json::json!({
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": {
                        "type": "eq",
                        "term": "event_id",
                        "value": "evt-1"
                    },
                    "max-credential-ttl-seconds": 300
                },
                "lakecat:raw-credential-exception": {
                    "requested": true,
                    "allowed": false,
                    "reason": "fine-grained read restriction requires Sail-planned reads"
                }
            }),
            checked_at: chrono::Utc::now(),
        };

        let payload = credentials_vend_audit_payload(&ident, &table, &profile, 1, &receipt);
        assert_eq!(
            payload["lakecat:raw-credential-exception"]["allowed"],
            serde_json::json!(false)
        );
        assert_eq!(
            payload["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            payload["read-restriction"]["row-predicate"],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );
        assert_eq!(
            payload["read-restriction"]["max-credential-ttl-seconds"],
            serde_json::json!(300)
        );
        assert_eq!(
            payload["authorization-receipt"]["context"]["read-restriction"],
            payload["read-restriction"]
        );
    }

    #[test]
    fn scan_planned_audit_payload_surfaces_policy_context() {
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let table = TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({ "format-version": 3 }),
            Principal::anonymous(),
        );
        let receipt = AuthorizationReceipt {
            principal: Principal::new("did:example:agent", PrincipalKind::Agent).unwrap(),
            action: CatalogAction::TablePlanScan,
            table: Some(ident.clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: Some("policy-hash".to_string()),
            context: serde_json::json!({
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": {
                        "type": "eq",
                        "term": "event_id",
                        "value": "evt-1"
                    }
                }
            }),
            checked_at: chrono::Utc::now(),
        };
        let scan = lakecat_sail::ScanPlan {
            planned_by: "lakecat-sail".to_string(),
            snapshot_id: Some(42),
            scan_tasks: vec![serde_json::json!({"task": 1})],
            residual_filter: None,
        };

        let payload = table_scan_planned_audit_payload(&ident, &table, &receipt, &scan);
        assert_eq!(
            payload["storage-location"],
            serde_json::json!("file:///tmp/events")
        );
        assert_eq!(
            payload["metadata-location"],
            serde_json::json!("file:///tmp/events/metadata/00000.json")
        );
        assert_eq!(
            payload["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            payload["authorization-receipt"]["context"]["read-restriction"],
            payload["read-restriction"]
        );
        assert_eq!(payload["scan-task-count"], serde_json::json!(1));
    }

    #[test]
    fn scan_tasks_fetched_audit_payload_surfaces_policy_context() {
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let table = TableRecord::new(
            ident.clone(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({ "format-version": 3 }),
            Principal::anonymous(),
        );
        let receipt = AuthorizationReceipt {
            principal: Principal::new("did:example:agent", PrincipalKind::Agent).unwrap(),
            action: CatalogAction::TablePlanScan,
            table: Some(ident.clone()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: Some("policy-hash".to_string()),
            context: serde_json::json!({
                "read-restriction": {
                    "allowed-columns": ["event_id"],
                    "row-predicate": {
                        "type": "eq",
                        "term": "event_id",
                        "value": "evt-1"
                    }
                }
            }),
            checked_at: chrono::Utc::now(),
        };
        let fetched = lakecat_sail::FetchScanTasksPlan {
            planned_by: "lakecat-sail".to_string(),
            plan_task: "lakecat:plan:abc".to_string(),
            snapshot_id: Some(42),
            file_scan_tasks: vec![serde_json::json!({"file": "events.parquet"})],
            delete_files: vec![serde_json::json!({"file": "events-delete.parquet"})],
            plan_tasks: vec![serde_json::json!({"task": 2})],
            residual_filter: None,
        };

        let payload = table_scan_tasks_fetched_audit_payload(&ident, &table, &receipt, &fetched);
        assert_eq!(
            payload["storage-location"],
            serde_json::json!("file:///tmp/events")
        );
        assert_eq!(
            payload["metadata-location"],
            serde_json::json!("file:///tmp/events/metadata/00000.json")
        );
        assert_eq!(
            payload["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            payload["authorization-receipt"]["context"]["read-restriction"],
            payload["read-restriction"]
        );
        assert_eq!(payload["file-scan-task-count"], serde_json::json!(1));
        assert_eq!(payload["delete-file-count"], serde_json::json!(1));
        assert_eq!(payload["child-plan-task-count"], serde_json::json!(1));
    }

    #[test]
    fn effective_projection_cannot_widen_policy_columns() {
        let restriction = ReadRestriction {
            allowed_columns: Some(vec!["event_id".to_string()]),
            ..ReadRestriction::unrestricted()
        };
        assert_eq!(
            restriction.effective_projection(&[]).unwrap(),
            vec!["event_id".to_string()]
        );
        assert_eq!(
            restriction
                .effective_projection(&["event_id".to_string(), "payload".to_string()])
                .unwrap(),
            vec!["event_id".to_string()]
        );
        assert!(
            restriction
                .effective_projection(&["payload".to_string()])
                .is_err()
        );
    }

    #[cfg(feature = "sail-local")]
    #[tokio::test]
    async fn scan_planning_applies_policy_column_restriction_before_sail() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/policies/agent-columns")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "namespace": ["default"],
                    "table": "events",
                    "enforced": true,
                    "odrl": {
                        "uid": "policy:agent-columns",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"],
                            "row-predicate": {
                                "type": "eq",
                                "term": "event_id",
                                "value": "evt-1"
                            }
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":2,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let plan = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::from(
                serde_json::json!({
                    "select": ["event_id", "payload"],
                    "case-sensitive": true
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(plan).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["residual-filter"]["select"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["requested-projection"],
            serde_json::json!(["event_id", "payload"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["effective-projection"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["read-restriction"]["row-predicate"],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );
        assert_eq!(
            body["residual-filter"]["filters-accepted-by-sail"][0]["filter"],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );
        let plan_task = body["plan-tasks"][0].as_str().unwrap().to_string();

        let fetch = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/tasks")
            .header("content-type", "application/json")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::from(
                serde_json::json!({ "plan-task": plan_task }).to_string(),
            ))
            .unwrap();
        let response = app.oneshot(fetch).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["residual-filter"]["projection"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["residual-filter"]["filters"][0],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );
    }

    #[cfg(feature = "sail-local")]
    #[tokio::test]
    async fn stale_commit_requirement_returns_conflict_with_sail_local_engine() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true,"doc":"Event identifier."}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"requirements":[{"type":"assert-current-schema-id","current-schema-id":9}],"updates":[]}"#,
            ))
            .unwrap();
        let response = app.oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[cfg(feature = "sail-local")]
    #[tokio::test]
    async fn stale_commit_cleans_up_uncommitted_metadata_file() {
        let app = test_app();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-orphan-cleanup-{unique}"));
        let table_dir = root.join("events");
        let metadata_dir = table_dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let table_location = url::Url::from_directory_path(&table_dir)
            .expect("table dir URL")
            .to_string();
        let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
            .unwrap()
            .to_string();
        let rejected_metadata_path = metadata_dir.join("00001.json");
        let rejected_metadata_location = url::Url::from_file_path(&rejected_metadata_path)
            .unwrap()
            .to_string();
        let base_metadata = serde_json::json!({
            "format-version": 3,
            "table-uuid": "11111111-1111-1111-1111-111111111111",
            "location": table_location,
            "last-sequence-number": 7,
            "last-updated-ms": 1710000000000_i64,
            "last-column-id": 1,
            "schemas": [{
                "type": "struct",
                "schema-id": 1,
                "fields": [{
                    "id": 1,
                    "name": "id",
                    "type": "string",
                    "required": true
                }]
            }],
            "current-schema-id": 1,
            "partition-specs": [{"spec-id": 0, "fields": []}],
            "default-spec-id": 0,
            "current-snapshot-id": 42,
            "snapshots": [{
                "snapshot-id": 42,
                "sequence-number": 7,
                "timestamp-ms": 1710000000000_i64,
                "summary": {"operation": "append"},
                "schema-id": 1
            }],
            "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
        });
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "name": "events",
                    "location": table_location,
                    "metadata-location": initial_metadata_location,
                    "metadata": base_metadata,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let mut rejected_metadata = base_metadata;
        rejected_metadata["last-sequence-number"] = serde_json::json!(8);
        rejected_metadata["last-updated-ms"] = serde_json::json!(1710000000100_i64);
        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "requirements": [{
                        "type": "assert-current-schema-id",
                        "current-schema-id": 9
                    }],
                    "updates": [],
                    "metadata-location": rejected_metadata_location,
                    "metadata": rejected_metadata,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(!rejected_metadata_path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    fn test_app() -> Router {
        app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        ))
    }

    #[cfg(feature = "sail-local")]
    struct LocalManifestFixture {
        root: std::path::PathBuf,
        table_location: String,
        metadata_location: String,
        delete_file_path: String,
        metadata: serde_json::Value,
    }

    #[cfg(feature = "sail-local")]
    fn local_manifest_fixture() -> LocalManifestFixture {
        use std::collections::HashMap;
        use std::sync::Arc;
        use std::time::{SystemTime, UNIX_EPOCH};

        use sail_iceberg::spec::{
            DataContentType, DataFile, DataFileFormat, FormatVersion, ManifestContentType,
            ManifestFile, ManifestListWriter, ManifestMetadata, ManifestWriterBuilder,
            TableMetadata,
        };
        use url::Url;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-service-manifest-{unique}"));
        let table_dir = root.join("table");
        let metadata_dir = table_dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();

        let table_location = Url::from_directory_path(&table_dir).unwrap().to_string();
        let manifest_list_path = metadata_dir.join("snap-42.avro");
        let manifest_list = Url::from_file_path(&manifest_list_path)
            .unwrap()
            .to_string();
        let metadata_location = format!("{table_location}metadata/00000.json");
        let manifest_path = Url::from_file_path(metadata_dir.join("manifest-1.avro"))
            .unwrap()
            .to_string();
        let delete_manifest_path = Url::from_file_path(metadata_dir.join("delete-manifest-1.avro"))
            .unwrap()
            .to_string();
        let data_file_path = Url::from_file_path(table_dir.join("data").join("part-1.parquet"))
            .unwrap()
            .to_string();
        let delete_file_path =
            Url::from_file_path(table_dir.join("delete").join("pos-delete-1.parquet"))
                .unwrap()
                .to_string();
        let metadata = serde_json::json!({
            "format-version": 3,
            "table-uuid": "11111111-1111-1111-1111-111111111111",
            "location": table_location,
            "last-sequence-number": 8,
            "last-updated-ms": 1710000000000_i64,
            "last-column-id": 1,
            "schemas": [{
                "type": "struct",
                "schema-id": 1,
                "fields": [{
                    "id": 1,
                    "name": "id",
                    "type": "string",
                    "required": true,
                    "doc": "Event identifier."
                }]
            }],
            "current-schema-id": 1,
            "partition-specs": [{"spec-id": 0, "fields": []}],
            "default-spec-id": 0,
            "current-snapshot-id": 42,
            "snapshots": [{
                "snapshot-id": 42,
                "sequence-number": 8,
                "timestamp-ms": 1710000000000_i64,
                "manifest-list": manifest_list,
                "summary": {"operation": "append"},
                "schema-id": 1
            }],
            "snapshot-log": [{
                "timestamp-ms": 1710000000000_i64,
                "snapshot-id": 42
            }]
        });
        let table_metadata =
            TableMetadata::from_json(&serde_json::to_vec(&metadata).unwrap()).unwrap();
        let data_manifest_metadata = ManifestMetadata::new(
            Arc::new(table_metadata.current_schema().unwrap().clone()),
            table_metadata.current_schema_id,
            table_metadata.default_partition_spec().unwrap().clone(),
            FormatVersion::V2,
            ManifestContentType::Data,
        );
        let mut data_writer =
            ManifestWriterBuilder::new(Some(42), None, data_manifest_metadata).build();
        data_writer.add(DataFile {
            content: DataContentType::Data,
            file_path: data_file_path,
            file_format: DataFileFormat::Parquet,
            partition: Vec::new(),
            record_count: 3,
            file_size_in_bytes: 123,
            column_sizes: HashMap::new(),
            value_counts: HashMap::new(),
            null_value_counts: HashMap::new(),
            nan_value_counts: HashMap::new(),
            lower_bounds: HashMap::new(),
            upper_bounds: HashMap::new(),
            block_size_in_bytes: None,
            key_metadata: None,
            split_offsets: Vec::new(),
            equality_ids: Vec::new(),
            sort_order_id: None,
            first_row_id: None,
            partition_spec_id: 0,
            referenced_data_file: None,
            content_offset: None,
            content_size_in_bytes: None,
        });
        std::fs::write(
            Url::parse(&manifest_path).unwrap().to_file_path().unwrap(),
            data_writer.to_avro_bytes_v2().unwrap(),
        )
        .unwrap();

        let delete_manifest_metadata = ManifestMetadata::new(
            Arc::new(table_metadata.current_schema().unwrap().clone()),
            table_metadata.current_schema_id,
            table_metadata.default_partition_spec().unwrap().clone(),
            FormatVersion::V2,
            ManifestContentType::Deletes,
        );
        let mut delete_writer =
            ManifestWriterBuilder::new(Some(42), None, delete_manifest_metadata).build();
        delete_writer.add(DataFile {
            content: DataContentType::PositionDeletes,
            file_path: delete_file_path.clone(),
            file_format: DataFileFormat::Parquet,
            partition: Vec::new(),
            record_count: 1,
            file_size_in_bytes: 64,
            column_sizes: HashMap::new(),
            value_counts: HashMap::new(),
            null_value_counts: HashMap::new(),
            nan_value_counts: HashMap::new(),
            lower_bounds: HashMap::new(),
            upper_bounds: HashMap::new(),
            block_size_in_bytes: None,
            key_metadata: None,
            split_offsets: Vec::new(),
            equality_ids: Vec::new(),
            sort_order_id: None,
            first_row_id: None,
            partition_spec_id: 0,
            referenced_data_file: Some(
                Url::from_file_path(table_dir.join("data").join("part-1.parquet"))
                    .unwrap()
                    .to_string(),
            ),
            content_offset: None,
            content_size_in_bytes: None,
        });
        std::fs::write(
            Url::parse(&delete_manifest_path)
                .unwrap()
                .to_file_path()
                .unwrap(),
            delete_writer.to_avro_bytes_v2().unwrap(),
        )
        .unwrap();

        let mut list_writer = ManifestListWriter::new();
        list_writer.append(
            ManifestFile::builder()
                .with_manifest_path(&manifest_path)
                .with_manifest_length(10)
                .with_partition_spec_id(0)
                .with_content(ManifestContentType::Data)
                .with_sequence_number(7)
                .with_min_sequence_number(7)
                .with_added_snapshot_id(42)
                .with_file_counts(1, 0, 0)
                .with_row_counts(3, 0, 0)
                .build()
                .unwrap(),
        );
        list_writer.append(
            ManifestFile::builder()
                .with_manifest_path(&delete_manifest_path)
                .with_manifest_length(10)
                .with_partition_spec_id(0)
                .with_content(ManifestContentType::Deletes)
                .with_sequence_number(8)
                .with_min_sequence_number(8)
                .with_added_snapshot_id(42)
                .with_file_counts(1, 0, 0)
                .with_row_counts(1, 0, 0)
                .build()
                .unwrap(),
        );
        std::fs::write(
            &manifest_list_path,
            list_writer.to_bytes(FormatVersion::V2).unwrap(),
        )
        .unwrap();

        LocalManifestFixture {
            root,
            table_location,
            metadata_location,
            delete_file_path,
            metadata,
        }
    }
}
