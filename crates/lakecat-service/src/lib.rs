use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use lakecat_api::{
    CatalogConfigResponse, CommitTableRequest, CommitTableResponse, ConfigEntry,
    CreateNamespaceRequest, CreateTableRequest, FetchScanTasksRequest as ApiFetchScanTasksRequest,
    FetchScanTasksResponse, LineageDrainEventSummary, LineageDrainResponse, ListNamespacesResponse,
    ListPolicyBindingsResponse, ListProjectsResponse, ListServersResponse,
    ListStorageProfilesResponse, ListTableCommitRecordsResponse,
    ListViewVersionReceiptChainsResponse, ListViewVersionReceiptsResponse, ListViewsResponse,
    ListWarehousesResponse, LoadCredentialsResponse, LoadTableResponse, NamespaceResponse,
    PlanTableScanRequest, PlanTableScanResponse, PolicyBindingResponse, ProjectResponse,
    ServerResponse, StorageCredential, StorageProfileResponse, TableCommitRecordResponse,
    TableIdentifier, UpsertPolicyBindingRequest, UpsertProjectRequest, UpsertServerRequest,
    UpsertStorageProfileRequest, UpsertViewRequest, UpsertWarehouseRequest, ViewColumnResponse,
    ViewResponse, ViewVersionReceiptChainResponse, ViewVersionReceiptResponse, WarehouseResponse,
};
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, PrincipalKind, TableIdent, TableName,
    WarehouseName, content_hash_bytes, content_hash_json,
};
use lakecat_graph::{CatalogGraphSink, GraphAction, GraphEvent, NoopCatalogGraphSink};
use lakecat_lineage::{
    HashOnlyLineageSink, LineageEvent, LineageEventType, LineageReceipt, LineageSink,
};
use lakecat_querygraph::{
    QueryGraphBootstrap, QueryGraphTenantProjection, QueryGraphViewReceiptEvidence,
};
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
    ProjectRecord, ServerRecord, StorageProfile, StorageProvider, TableCommit, TableCommitRecord,
    TableRecord, ViewColumnRecord, ViewRecord, ViewVersionOperation, ViewVersionReceipt,
    WarehouseRecord, table_ident,
};
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, PutMode, PutPayload};
use serde::Deserialize;
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ViewMutationQuery {
    #[serde(default)]
    expected_view_version: Option<u64>,
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

#[cfg(feature = "typesec-local")]
pub mod typesec_credential_issuer {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use lakecat_api::{ConfigEntry, StorageCredential};
    use lakecat_core::{LakeCatError, LakeCatResult, content_hash_bytes};
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
                return crate::issued_credentials_for_profile(
                    crate::public_storage_credentials_for_profile(&request.profile),
                    &request.profile,
                    request.max_credential_ttl_seconds,
                );
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
                PolicyResult::Allow => crate::issued_credentials_for_profile(
                    self.resolver.resolve(&request).await?,
                    &request.profile,
                    request.max_credential_ttl_seconds,
                ),
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
        provider_backends: BTreeMap<SecretRefProvider, Arc<dyn SecretRefCredentialResolver>>,
    }

    impl ExternalSecretRefCredentialResolver {
        pub fn new() -> Arc<Self> {
            Arc::new(Self {
                env: EnvironmentSecretRefCredentialResolver::new(),
                vault: VaultSecretRefCredentialResolver::from_env(),
                provider_backends: BTreeMap::new(),
            })
        }

        pub fn with_provider_backends(
            provider_backends: BTreeMap<SecretRefProvider, Arc<dyn SecretRefCredentialResolver>>,
        ) -> Arc<Self> {
            Arc::new(Self {
                env: EnvironmentSecretRefCredentialResolver::new(),
                vault: VaultSecretRefCredentialResolver::from_env(),
                provider_backends,
            })
        }

        #[cfg(test)]
        pub fn with_env_reader(
            reader: impl Fn(&str) -> Result<String, std::env::VarError> + Send + Sync + 'static,
        ) -> Arc<Self> {
            Arc::new(Self {
                env: EnvironmentSecretRefCredentialResolver::with_reader(reader),
                vault: None,
                provider_backends: BTreeMap::new(),
            })
        }

        #[cfg(test)]
        pub fn with_vault(vault: Arc<VaultSecretRefCredentialResolver>) -> Arc<Self> {
            Arc::new(Self {
                env: EnvironmentSecretRefCredentialResolver::new(),
                vault: Some(vault),
                provider_backends: BTreeMap::new(),
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
                provider => {
                    let Some(backend) = self.provider_backends.get(&provider) else {
                        return Err(provider_not_configured(provider, secret_ref));
                    };
                    backend.resolve(request).await
                }
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum SecretRefProvider {
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
        let url = Url::parse(secret_ref).map_err(|_err| {
            LakeCatError::InvalidArgument(format!(
                "invalid credential secret ref URI; {}",
                secret_ref_hash_context(secret_ref)
            ))
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
             or configure a production secret-store backend; {}",
            provider.as_str(),
            secret_ref_hash_context(secret_ref),
        ))
    }

    pub(crate) fn vault_secret_path(secret_ref: &str) -> LakeCatResult<String> {
        let url = Url::parse(secret_ref).map_err(|_err| {
            LakeCatError::InvalidArgument(format!(
                "invalid Vault URI; {}",
                secret_ref_hash_context(secret_ref)
            ))
        })?;
        if url.scheme() != "vault" {
            return Err(LakeCatError::InvalidArgument(format!(
                "Vault resolver requires vault:// secret refs; {}",
                secret_ref_hash_context(secret_ref)
            )));
        }
        let Some(mount) = url.host_str() else {
            return Err(LakeCatError::InvalidArgument(format!(
                "Vault secret ref must include a mount name; {}",
                secret_ref_hash_context(secret_ref)
            )));
        };
        let path = url.path().trim_start_matches('/');
        if path.is_empty() {
            return Err(LakeCatError::InvalidArgument(format!(
                "Vault secret ref must include a secret path; {}",
                secret_ref_hash_context(secret_ref)
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
        let url = Url::parse(secret_ref).map_err(|_err| {
            LakeCatError::InvalidArgument(format!(
                "invalid TypeSec secret ref URI; {}",
                secret_ref_hash_context(secret_ref)
            ))
        })?;
        if url.scheme() != "typesec" || url.host_str() != Some("env") {
            return Err(LakeCatError::InvalidArgument(format!(
                "environment resolver requires secret refs like typesec://env/VARIABLE; {}",
                secret_ref_hash_context(secret_ref)
            )));
        }
        let variable = url.path().trim_start_matches('/');
        if variable.is_empty()
            || !variable
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err(LakeCatError::InvalidArgument(format!(
                "environment credential variable must be non-empty and use A-Z, 0-9, or _; {}",
                secret_ref_hash_context(secret_ref)
            )));
        }
        Ok(variable.to_string())
    }

    fn secret_ref_hash_context(secret_ref: &str) -> String {
        format!(
            "secret-ref-hash={}",
            content_hash_bytes(secret_ref.as_bytes())
        )
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
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/tables/{table}/commits",
            get(list_table_commits),
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
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/views/{view}/version-receipts",
            get(list_view_version_receipts),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/view-version-receipt-chains",
            get(list_view_version_receipt_chains),
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
    let mut seen_event_ids = BTreeSet::new();
    for event in &events {
        if !seen_event_ids.insert(event.event_id.as_str()) {
            return Err(LakeCatError::Conflict(format!(
                "outbox pending batch contained duplicate event id hash {}",
                content_hash_bytes(event.event_id.as_bytes())
            )));
        }
    }
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
    // Acknowledgement is all-or-retry: if any projection fails above, no pending
    // event is marked delivered and the outbox remains the recovery source.
    let projected = delivered.len();
    let delivered = state.store.mark_outbox_delivered(&delivered).await?;
    if delivered != projected {
        return Err(LakeCatError::Conflict(format!(
            "outbox drain acknowledgement mismatch: projected {projected} event(s) but marked {delivered} delivered"
        )));
    }
    Ok(LineageDrainResponse {
        delivered,
        event_types,
        graph_events,
        lineage_events,
        principal_subject: None,
        principal_kind: None,
        authorization_receipt_hash: None,
        request_identity_state: None,
        request_identity_source: None,
        typedid_envelope_hash: None,
        typedid_proof_hash: None,
        events: summaries,
    })
}

fn attach_lineage_drain_authorization(
    response: &mut LineageDrainResponse,
    receipt: &AuthorizationReceipt,
) -> Result<(), LakeCatError> {
    let receipt_value = serde_json::to_value(receipt).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "lineage read receipt was not JSON encodable: {err}"
        ))
    })?;
    response.principal_subject = Some(receipt.principal.subject.clone());
    response.principal_kind = receipt_value
        .pointer("/principal/kind")
        .and_then(Value::as_str)
        .map(str::to_string);
    response.authorization_receipt_hash = Some(content_hash_json(&receipt_value)?);
    response.request_identity_state = receipt
        .context
        .get("request-identity")
        .and_then(|identity| identity.get("attestation-state"))
        .and_then(Value::as_str)
        .map(str::to_string);
    response.request_identity_source = receipt
        .context
        .get("request-identity")
        .and_then(|identity| identity.get("source"))
        .and_then(Value::as_str)
        .map(str::to_string);
    response.typedid_envelope_hash = receipt
        .context
        .get("request-identity")
        .and_then(|identity| identity.get("typedid-envelope-sha256"))
        .and_then(Value::as_str)
        .map(str::to_string);
    response.typedid_proof_hash = receipt
        .context
        .get("request-identity")
        .and_then(|identity| identity.get("typedid-proof-sha256"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(())
}

fn lineage_drain_event_summary(
    event: &OutboxEvent,
    receipt: &OutboxProjectionReceipt,
) -> LineageDrainEventSummary {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let view = payload.get("view");
    let view_warehouse = view
        .and_then(|view| view.get("warehouse"))
        .or_else(|| payload.get("warehouse"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let view_namespace = namespace_summary_parts(
        view.and_then(|view| view.get("namespace"))
            .or_else(|| payload.get("namespace")),
    );
    let view_name = view
        .and_then(|view| view.get("name"))
        .and_then(Value::as_str)
        .or_else(|| payload.get("view").and_then(Value::as_str))
        .map(str::to_string);
    let view_name_ref = view_name.as_deref();
    let view_version = view
        .and_then(|view| view.get("view-version"))
        .and_then(Value::as_u64);
    let expected_view_version = payload.get("expected-view-version").and_then(Value::as_u64);
    let view_stable_id = match (
        view_warehouse.as_deref(),
        view_namespace.as_slice(),
        view_name_ref,
    ) {
        (Some(warehouse), namespace, Some(name)) if !namespace.is_empty() => Some(format!(
            "lakecat:view:{warehouse}:{}:{name}",
            namespace.join(".")
        )),
        _ => None,
    };
    let view_version_receipt_hashes = payload
        .get("view-version-receipts")
        .and_then(Value::as_array)
        .map(|receipts| {
            receipts
                .iter()
                .filter_map(|receipt| {
                    receipt
                        .get("receipt-hash")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .or_else(|| {
            payload.get("drop-receipt-hashes").and_then(|hashes| {
                hashes.as_array().map(|hashes| {
                    hashes
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
            })
        })
        .unwrap_or_default();
    let view_version_receipt_chain_hashes = payload
        .get("view-version-receipt-chains")
        .and_then(Value::as_array)
        .map(|chains| {
            chains
                .iter()
                .filter_map(|chain| {
                    chain
                        .get("chain-hash")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .or_else(|| {
            payload.get("chain-hashes").and_then(|hashes| {
                hashes.as_array().map(|hashes| {
                    hashes
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
            })
        })
        .unwrap_or_default();
    let view_version_receipt_chain_verified_count = payload
        .get("chain-verified-count")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .or_else(|| {
            payload
                .get("view-version-receipt-chains")
                .and_then(Value::as_array)
                .map(|chains| {
                    chains
                        .iter()
                        .filter(|chain| {
                            chain
                                .get("chain-verified")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                        })
                        .count()
                })
        })
        .unwrap_or_default();
    let storage_profile = payload.get("storage-profile");
    let storage_profile_secret_ref = storage_profile
        .and_then(|profile| profile.get("secret-ref"))
        .and_then(Value::as_str);
    let storage_profile_secret_ref_hash = storage_profile.and_then(|profile| {
        profile
            .get("secret-ref-hash")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                storage_profile_secret_ref
                    .map(|secret_ref| content_hash_bytes(secret_ref.as_bytes()))
            })
    });
    let storage_profile_location_prefix_hash = storage_profile.and_then(|profile| {
        profile
            .get("location-prefix-hash")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                profile
                    .get("location-prefix")
                    .and_then(Value::as_str)
                    .and_then(|location_prefix| {
                        content_hash_json(&json!({"location-prefix": location_prefix})).ok()
                    })
            })
    });
    let authorization_receipt = payload
        .get("authorization-receipt")
        .or_else(|| payload.pointer("/payload/authorization-receipt"));
    let request_identity = authorization_receipt
        .and_then(|receipt| receipt.get("request-identity"))
        .or_else(|| {
            authorization_receipt
                .and_then(|receipt| receipt.get("context"))
                .and_then(|context| context.get("request-identity"))
        });
    let raw_credential_exception_allowed = payload
        .pointer("/lakecat:raw-credential-exception/allowed")
        .and_then(Value::as_bool);
    let raw_credential_exception_reason = raw_credential_exception_allowed
        .filter(|allowed| *allowed)
        .and_then(|_| {
            payload
                .pointer("/lakecat:raw-credential-exception/reason")
                .and_then(Value::as_str)
        })
        .map(str::to_string);
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
        request_identity_state: request_identity
            .and_then(|identity| identity.get("attestation-state"))
            .and_then(Value::as_str)
            .map(str::to_string),
        request_identity_source: request_identity
            .and_then(|identity| identity.get("source"))
            .and_then(Value::as_str)
            .map(str::to_string),
        typedid_envelope_hash: request_identity
            .and_then(|identity| identity.get("typedid-envelope-sha256"))
            .and_then(Value::as_str)
            .map(str::to_string),
        typedid_proof_hash: request_identity
            .and_then(|identity| identity.get("typedid-proof-sha256"))
            .and_then(Value::as_str)
            .map(str::to_string),
        agent_delegation_hash: request_identity
            .and_then(|identity| identity.get("agent-delegation-sha256"))
            .and_then(Value::as_str)
            .map(str::to_string),
        agent_summary_signature_hash: request_identity
            .and_then(|identity| identity.get("agent-summary-signature-sha256"))
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
        view_version_receipt_hashes,
        view_version_receipt_chain_hashes,
        view_version_receipt_chain_verified_count,
        view_warehouse,
        view_namespace,
        view_name,
        view_stable_id,
        view_version,
        expected_view_version,
        policy_binding_count: payload
            .get("policy-binding-count")
            .or_else(|| payload.get("policy-count"))
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok())
            .unwrap_or_default(),
        project_count: payload
            .get("project-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        server_count: payload
            .get("server-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        storage_profile_count: payload
            .get("storage-profile-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        storage_profile_id: storage_profile
            .and_then(|profile| profile.get("profile-id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        storage_profile_provider: storage_profile
            .and_then(|profile| profile.get("provider"))
            .and_then(Value::as_str)
            .map(str::to_string),
        storage_profile_issuance_mode: storage_profile
            .and_then(|profile| profile.get("issuance-mode"))
            .and_then(Value::as_str)
            .map(str::to_string),
        storage_profile_location_prefix_hash,
        storage_profile_secret_ref_present: storage_profile
            .and_then(|profile| profile.get("secret-ref-present"))
            .and_then(Value::as_bool)
            .or_else(|| storage_profile_secret_ref.map(|_| true)),
        storage_profile_secret_ref_provider: storage_profile
            .and_then(|profile| profile.get("secret-ref-provider"))
            .and_then(Value::as_str)
            .or_else(|| storage_profile_secret_ref.and_then(secret_ref_provider_label))
            .map(str::to_string),
        storage_profile_secret_ref_hash,
        warehouse_count: payload
            .get("warehouse-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        table_commit_count: payload
            .get("commit-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        table_commit_sequence_numbers: payload
            .get("sequence-numbers")
            .and_then(Value::as_array)
            .map(|numbers| numbers.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default(),
        table_commit_hashes: payload
            .get("commit-hashes")
            .and_then(Value::as_array)
            .map(|hashes| {
                hashes
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        scan_task_count: payload
            .get("scan-task-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        file_scan_task_count: payload
            .get("file-scan-task-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        delete_file_count: payload
            .get("delete-file-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        child_plan_task_count: payload
            .get("child-plan-task-count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok()),
        read_restriction: payload.get("read-restriction").cloned(),
        required_projection: payload
            .get("required-projection")
            .and_then(Value::as_array)
            .map(|columns| {
                columns
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        required_filters: payload
            .get("required-filters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        requested_stats_fields: payload
            .get("requested-stats-fields")
            .and_then(Value::as_array)
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        effective_stats_fields: payload
            .get("effective-stats-fields")
            .and_then(Value::as_array)
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        management_scope_project_id: payload
            .get("project-id")
            .and_then(Value::as_str)
            .map(str::to_string),
        management_scope_warehouse: payload
            .get("warehouse")
            .and_then(Value::as_str)
            .map(str::to_string),
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
        raw_credential_exception_allowed,
        raw_credential_exception_reason,
        replay_event_hashes: receipt.lineage_event_hashes.clone(),
        replay_open_lineage_hashes: receipt.open_lineage_hashes.clone(),
    }
}

fn namespace_summary_parts(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(path)) => path.split('.').map(str::to_string).collect(),
        _ => Vec::new(),
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

async fn list_table_commits(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<Json<ListTableCommitRecordsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability =
        authorize_table_load(&state, request_identity(&headers)?, ident.clone()).await?;
    state.store.load_table(capability.table()).await?;
    let records = state
        .store
        .table_commit_records(capability.table(), 0, None)
        .await?;
    let commits = records
        .iter()
        .map(table_commit_record_response)
        .collect::<LakeCatResult<Vec<_>>>()?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.commits-listed",
            Some(capability.table().clone()),
            capability.receipt().principal.clone(),
            json!({
                "event-type": "table.commits-listed",
                "warehouse": capability.table().warehouse.as_str(),
                "namespace": capability.table().namespace.parts(),
                "table": capability.table().name.as_str(),
                "commit-count": commits.len(),
                "commit-hashes": commits
                    .iter()
                    .map(|commit| commit.commit_hash.clone())
                    .collect::<Vec<_>>(),
                "sequence-numbers": commits
                    .iter()
                    .map(|commit| commit.sequence_number)
                    .collect::<Vec<_>>(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListTableCommitRecordsResponse { commits }))
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
    let max_credential_ttl_seconds = read_restriction.max_credential_ttl_seconds;
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
        canonicalize_credential_response_evidence(
            issued_credentials_for_profile(
                state
                    .credential_issuer
                    .issue(CredentialIssuanceRequest {
                        table: table.clone(),
                        profile: storage_profile.clone(),
                        authorization_receipt: capability.receipt().clone(),
                        max_credential_ttl_seconds,
                    })
                    .await?,
                &storage_profile,
                max_credential_ttl_seconds,
            )?,
            &storage_profile,
            capability.receipt(),
            read_restriction.requires_governed_read(),
            max_credential_ttl_seconds,
        )
    };
    let ident = capability.table().clone();
    let mut audit_payload = credentials_vend_audit_payload(
        &ident,
        &table,
        &storage_profile,
        &storage_credentials,
        capability.receipt(),
    )?;
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
    storage_credentials: &[StorageCredential],
    receipt: &AuthorizationReceipt,
) -> LakeCatResult<Value> {
    let mut audit_payload = json!({
        "event-type": "credentials.vend-attempted",
        "table": ident.clone(),
        "authorization-receipt": receipt,
        "storage-location": table.location,
        "storage-profile-id": storage_profile.profile_id,
        "storage-profile": {
            "profile-id": storage_profile.profile_id,
            "warehouse": storage_profile.warehouse.as_str(),
            "provider": storage_profile.provider.as_str(),
            "issuance-mode": storage_profile.issuance_mode.as_str(),
            "secret-ref-present": storage_profile.secret_ref.is_some(),
            "location-prefix-hash": content_hash_json(&json!({
                "location-prefix": storage_profile.location_prefix
            }))?,
        },
        "secret-ref-present": storage_profile.secret_ref.is_some(),
        "credential-count": storage_credentials.len(),
        "credential-response-evidence": credential_response_evidence(
            storage_credentials,
            storage_profile,
            receipt
        )?,
        "mode": storage_profile.issuance_mode.as_str(),
    });
    if let Some(restriction) = receipt.context.get("read-restriction") {
        audit_payload["read-restriction"] = restriction.clone();
    }
    if let Some(exception) = receipt.context.get("lakecat:raw-credential-exception") {
        audit_payload["lakecat:raw-credential-exception"] = exception.clone();
    }
    Ok(audit_payload)
}

fn credential_response_evidence(
    storage_credentials: &[StorageCredential],
    storage_profile: &StorageProfile,
    receipt: &AuthorizationReceipt,
) -> LakeCatResult<Value> {
    Ok(Value::Array(
        storage_credentials
            .iter()
            .map(|credential| {
                let non_lakecat_config = credential
                    .config
                    .iter()
                    .filter(|entry| !entry.key.to_ascii_lowercase().starts_with("lakecat."))
                    .cloned()
                    .collect::<Vec<_>>();
                let non_lakecat_config =
                    serde_json::to_value(non_lakecat_config).map_err(|err| {
                        LakeCatError::Internal(format!(
                            "failed to serialize credential response evidence: {err}"
                        ))
                    })?;
                Ok(json!({
                    "prefix-hash": content_hash_json(&json!({
                        "credential-prefix": &credential.prefix
                    }))?,
                    "storage-profile-id": single_config_value(
                        &credential.config,
                        "lakecat.storage-profile-id"
                    ),
                    "storage-provider": single_config_value(
                        &credential.config,
                        "lakecat.storage-provider"
                    ),
                    "credential-mode": single_config_value(
                        &credential.config,
                        "lakecat.credential-mode"
                    ),
                    "authorization-principal": single_config_value(
                        &credential.config,
                        "lakecat.authorization-principal"
                    ),
                    "governed-read-required": single_config_value(
                        &credential.config,
                        "lakecat.governed-read-required"
                    ),
                    "max-credential-ttl-seconds": single_config_value(
                        &credential.config,
                        "lakecat.max-credential-ttl-seconds"
                    ),
                    "issuer-config-entry-count": non_lakecat_config
                        .as_array()
                        .map_or(0, Vec::len),
                    "issuer-config-hash": content_hash_json(&non_lakecat_config)?,
                    "catalog-profile-id": &storage_profile.profile_id,
                    "receipt-principal": &receipt.principal.subject,
                }))
            })
            .collect::<LakeCatResult<Vec<_>>>()?,
    ))
}

fn single_config_value(config: &[ConfigEntry], key: &str) -> Option<String> {
    let mut values = config
        .iter()
        .filter(|entry| entry.key == key)
        .map(|entry| entry.value.clone());
    let first = values.next()?;
    values.next().is_none().then_some(first)
}

fn table_scan_planned_audit_payload(
    ident: &TableIdent,
    table: &TableRecord,
    receipt: &AuthorizationReceipt,
    scan: &lakecat_sail::ScanPlan,
    scan_request_extensions: &Value,
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
        append_read_restriction_requirements(&mut audit_payload, restriction);
    }
    for field in ["requested-stats-fields", "effective-stats-fields"] {
        if let Some(value) = scan_request_extensions.get(field) {
            audit_payload[field] = value.clone();
        }
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
        append_read_restriction_requirements(&mut audit_payload, restriction);
    }
    audit_payload
}

fn append_read_restriction_requirements(audit_payload: &mut Value, restriction: &Value) {
    if let Ok(restriction) = serde_json::from_value::<ReadRestriction>(restriction.clone()) {
        if let Ok(required_projection) = restriction.effective_projection(&[]) {
            audit_payload["required-projection"] = json!(required_projection);
            audit_payload["required-filters"] = json!(restriction.mandatory_filters());
        }
    }
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
                "storage-profile": storage_profile_event_payload(&storage_profile),
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

async fn list_view_version_receipts(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
) -> Result<Json<ListViewVersionReceiptsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_load(&state, request_identity(&headers)?).await?;
    let receipts = state
        .store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await?;
    let response_receipts = receipts
        .iter()
        .map(view_version_receipt_response)
        .collect::<LakeCatResult<Vec<_>>>()?;
    let drop_receipt_hashes = response_receipts
        .iter()
        .filter(|receipt| receipt.operation == "drop")
        .map(|receipt| receipt.receipt_hash.clone())
        .collect::<Vec<_>>();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.version-receipts-listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.version-receipts-listed",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view": view_name.as_str(),
                "receipt-count": response_receipts.len(),
                "receipt-hashes": response_receipts
                    .iter()
                    .map(|receipt| receipt.receipt_hash.clone())
                    .collect::<Vec<_>>(),
                "drop-receipt-hashes": drop_receipt_hashes,
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListViewVersionReceiptsResponse {
        receipts: response_receipts,
    }))
}

async fn list_view_version_receipt_chains(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<Json<ListViewVersionReceiptChainsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_load(&state, request_identity(&headers)?).await?;
    let receipts = state
        .store
        .list_namespace_view_version_receipts(&warehouse, &namespace)
        .await?;
    let chains = view_version_receipt_chains(&receipts)?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.version-receipt-chains-listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.version-receipt-chains-listed",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "chain-count": chains.len(),
                "receipt-count": chains.iter().map(|chain| chain.receipt_count).sum::<usize>(),
                "tombstone-count": chains.iter().filter(|chain| chain.tombstoned).count(),
                "chain-verified-count": chains.iter().filter(|chain| chain.chain_verified).count(),
                "view-version-receipt-chains": chains,
                "chain-hashes": chains
                    .iter()
                    .map(|chain| chain.chain_hash.clone())
                    .collect::<Vec<_>>(),
                "receipt-hashes": chains
                    .iter()
                    .flat_map(|chain| chain.receipts.iter().map(|receipt| receipt.receipt_hash.clone()))
                    .collect::<Vec<_>>(),
                "drop-receipt-hashes": chains
                    .iter()
                    .flat_map(|chain| {
                        chain
                            .receipts
                            .iter()
                            .filter(|receipt| receipt.operation == "drop")
                            .map(|receipt| receipt.receipt_hash.clone())
                    })
                    .collect::<Vec<_>>(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListViewVersionReceiptChainsResponse { chains }))
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
    let record = state
        .store
        .upsert_view_if_version(record, request.expected_view_version)
        .await?;
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
                "expected-view-version": request.expected_view_version,
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
    Query(query): Query<ViewMutationQuery>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_drop(&state, request_identity(&headers)?).await?;
    let record = state
        .store
        .drop_view_if_version(
            &warehouse,
            &namespace,
            &view_name,
            capability.receipt().principal.clone(),
            query.expected_view_version,
        )
        .await?;
    record_view_drop_audit(
        &state,
        &warehouse,
        &namespace,
        &record,
        &capability,
        None,
        query.expected_view_version,
    )
    .await?;
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
    let record = state
        .store
        .upsert_view_if_version(record, request.expected_view_version)
        .await?;
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
                "expected-view-version": request.expected_view_version,
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
    Query(query): Query<ViewMutationQuery>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_drop(&state, request_identity(&headers)?).await?;
    let record = state
        .store
        .drop_view_if_version(
            &warehouse,
            &namespace,
            &view_name,
            capability.receipt().principal.clone(),
            query.expected_view_version,
        )
        .await?;
    record_view_drop_audit(
        &state,
        &warehouse,
        &namespace,
        &record,
        &capability,
        Some("iceberg-rest"),
        query.expected_view_version,
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
    expected_view_version: Option<u64>,
) -> Result<(), LakeCatHttpError> {
    let mut payload = json!({
        "event-type": "view.dropped",
        "warehouse": warehouse.as_str(),
        "namespace": namespace.parts(),
        "view": view_response(record),
        "expected-view-version": expected_view_version,
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
    if let (Some(idempotency_key), Some(idempotency_request_hash)) =
        (&idempotency_key, &idempotency_request_hash)
        && let Some(table) = state
            .store
            .replay_table_commit(
                capability.table(),
                idempotency_key,
                idempotency_request_hash,
            )
            .await?
    {
        return Ok(Json(CommitTableResponse {
            metadata_location: table.metadata_location,
            metadata: table.metadata,
        }));
    }
    let current = state.store.load_table(capability.table()).await?;
    let storage_profile = state.store.storage_profile_for_table(&current).await?;
    let current_metadata_location = current.metadata_location.clone();
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
    validate_planned_metadata_location(
        &commit_plan,
        current_metadata_location.as_deref(),
        &storage_profile,
    )?;
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
            let err = cleanup_planned_metadata_after_commit_error(
                metadata_write,
                current_metadata_location.as_deref(),
                err,
            )
            .await;
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
        LakeCatError::InvalidArgument(format!(
            "invalid metadata location {}; {}",
            metadata_location_hash_context(location),
            error_detail_hash_context(err)
        ))
    })?;
    object_store::parse_url_opts(&url, std::env::vars()).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "metadata object location {} is not supported or is not configured: {}",
            metadata_location_hash_context(location),
            error_detail_hash_context(err)
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
        .put_opts(
            &object_path,
            PutPayload::from(payload),
            PutMode::Create.into(),
        )
        .await
        .map_err(|err| match err {
            object_store::Error::AlreadyExists { .. } => LakeCatError::Conflict(format!(
                "metadata object {} already exists; refusing to overwrite existing metadata",
                metadata_location_hash_context(location)
            )),
            err => metadata_object_write_error(location, err),
        })?;
    Ok(Some(PlannedMetadataWrite {
        location: location.to_string(),
    }))
}

fn validate_planned_metadata_location(
    commit_plan: &lakecat_sail::CommitPlan,
    current_metadata_location: Option<&str>,
    storage_profile: &StorageProfile,
) -> Result<(), LakeCatError> {
    if !commit_plan.metadata_write_required {
        return Ok(());
    }
    let Some(new_metadata_location) = commit_plan.new_metadata_location.as_deref() else {
        return Err(LakeCatError::InvalidArgument(
            "metadata object commit requires a new metadata location".to_string(),
        ));
    };
    if current_metadata_location == Some(new_metadata_location) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object commit must not overwrite the current metadata location; {}",
            metadata_location_hash_context(new_metadata_location)
        )));
    }
    if location_has_dot_path_segment(new_metadata_location) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object location {} must not contain dot path segments",
            metadata_location_hash_context(new_metadata_location)
        )));
    }
    if location_has_query_or_fragment(new_metadata_location) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object location {} must not include query strings or fragments",
            metadata_location_hash_context(new_metadata_location)
        )));
    }
    if location_has_userinfo(new_metadata_location) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object location {} must not include userinfo",
            metadata_location_hash_context(new_metadata_location)
        )));
    }
    if !location_is_strictly_within_prefix(
        new_metadata_location,
        storage_profile.location_prefix.as_str(),
    ) {
        return Err(LakeCatError::InvalidArgument(format!(
            "metadata object location {} is outside storage profile '{}' prefix or is not a child object; storage-profile-prefix-hash={}",
            metadata_location_hash_context(new_metadata_location),
            storage_profile.profile_id,
            content_hash_bytes(storage_profile.location_prefix.as_bytes())
        )));
    }
    Ok(())
}

fn location_has_dot_path_segment(location: &str) -> bool {
    let path = location
        .split_once(['?', '#'])
        .map_or(location, |(path, _)| path);
    path.split('/').any(is_dot_path_segment)
}

fn location_has_query_or_fragment(location: &str) -> bool {
    Url::parse(location)
        .map(|url| url.query().is_some() || url.fragment().is_some())
        .unwrap_or_else(|_| location.contains(['?', '#']))
}

fn location_has_userinfo(location: &str) -> bool {
    Url::parse(location)
        .map(|url| !url.username().is_empty() || url.password().is_some())
        .unwrap_or(false)
}

fn is_dot_path_segment(segment: &str) -> bool {
    let Some(decoded) = percent_decode_segment(segment) else {
        return segment == "." || segment == "..";
    };
    decoded.as_slice() == b"." || decoded.as_slice() == b".."
}

fn percent_decode_segment(segment: &str) -> Option<Vec<u8>> {
    if !segment.as_bytes().contains(&b'%') {
        return None;
    }
    let mut decoded = Vec::with_capacity(segment.len());
    let bytes = segment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let Some(high) = hex_value(bytes[index + 1]) else {
                decoded.push(bytes[index]);
                index += 1;
                continue;
            };
            let Some(low) = hex_value(bytes[index + 2]) else {
                decoded.push(bytes[index]);
                index += 1;
                continue;
            };
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Some(decoded)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn location_is_within_prefix(location: &str, prefix: &str) -> bool {
    if location == prefix {
        return true;
    }
    location_is_strictly_within_prefix(location, prefix)
}

fn location_is_strictly_within_prefix(location: &str, prefix: &str) -> bool {
    if location == prefix {
        return false;
    }
    if prefix.ends_with('/') {
        location.starts_with(prefix)
    } else {
        location
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
    }
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
    match store.delete(&object_path).await {
        Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
        Err(err) => Err(metadata_cleanup_error(&write.location, err)),
    }
}

fn metadata_object_write_error(
    metadata_location: &str,
    err: impl std::fmt::Display,
) -> LakeCatError {
    LakeCatError::Internal(format!(
        "failed to write metadata object {}: {}",
        metadata_location_hash_context(metadata_location),
        error_detail_hash_context(err)
    ))
}

fn metadata_cleanup_error(metadata_location: &str, err: impl std::fmt::Display) -> LakeCatError {
    LakeCatError::Internal(format!(
        "failed to clean up uncommitted metadata object {}: {}",
        metadata_location_hash_context(metadata_location),
        error_detail_hash_context(err)
    ))
}

fn metadata_location_hash_context(metadata_location: &str) -> String {
    format!(
        "metadata-location-hash={}",
        content_hash_bytes(metadata_location.as_bytes())
    )
}

fn error_detail_hash_context(err: impl std::fmt::Display) -> String {
    format!(
        "error-detail-hash={}",
        content_hash_bytes(err.to_string().as_bytes())
    )
}

async fn cleanup_planned_metadata_after_commit_error(
    write: Option<PlannedMetadataWrite>,
    previous_metadata_location: Option<&str>,
    commit_error: LakeCatError,
) -> LakeCatError {
    match cleanup_planned_metadata(write, previous_metadata_location).await {
        Ok(()) => commit_error,
        Err(cleanup_error) => commit_error_with_cleanup_failure(commit_error, cleanup_error),
    }
}

fn commit_error_with_cleanup_failure(
    commit_error: LakeCatError,
    cleanup_error: LakeCatError,
) -> LakeCatError {
    let cleanup_context = format!("metadata cleanup also failed: {cleanup_error}");
    match commit_error {
        LakeCatError::InvalidArgument(message) => {
            LakeCatError::InvalidArgument(format!("{message}; {cleanup_context}"))
        }
        LakeCatError::NotFound { object, name } => LakeCatError::NotFound {
            object,
            name: format!("{name}; {cleanup_context}"),
        },
        LakeCatError::Conflict(message) => {
            LakeCatError::Conflict(format!("{message}; {cleanup_context}"))
        }
        LakeCatError::NotSupported(message) => {
            LakeCatError::NotSupported(format!("{message}; {cleanup_context}"))
        }
        LakeCatError::Internal(message) => {
            LakeCatError::Internal(format!("{message}; {cleanup_context}"))
        }
    }
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
    let audit_payload = table_scan_planned_audit_payload(
        &ident,
        &table,
        capability.receipt(),
        &scan,
        &scan_request_extensions,
    );
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
        "requested-stats-fields": request.stats_fields,
        "effective-stats-fields": stats_fields,
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
            fetch_scan_tasks_extensions(&capability)?,
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
    capability: &TableScanCapability,
) -> Result<serde_json::Value, LakeCatHttpError> {
    let restriction = capability.read_restriction()?;
    let required_projection = restriction.effective_projection(&[])?;
    let required_filters = restriction.mandatory_filters();
    Ok(json!({
        "read-restriction": restriction,
        "required-projection": required_projection,
        "required-filters": required_filters,
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
    let tenant = querygraph_tenant_projection(&state).await?;
    let view_version_receipts = querygraph_view_version_receipts(&state, &views).await?;
    let bundle = QueryGraphBootstrap::from_tables_views_with_policy_bindings_and_tenant(
        state.warehouse.clone(),
        table_policy_bindings,
        views,
        tenant,
    )?
    .with_view_receipt_evidence(view_version_receipts)?;
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
                "verified-view-versions": verification.verified_view_versions,
                "view-version-receipts": verification
                    .verified_view_receipt_hashes
                    .iter()
                    .map(|(stable_id, receipt_hash)| json!({
                        "stable-id": stable_id,
                        "view-version": verification
                            .verified_view_versions
                            .get(stable_id)
                            .copied()
                            .unwrap_or_default(),
                        "receipt-hash": receipt_hash,
                        "receipt-chain-hash": verification
                            .verified_view_receipt_chain_hashes
                            .get(stable_id),
                    }))
                    .collect::<Vec<_>>(),
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

async fn querygraph_tenant_projection(
    state: &LakeCatState,
) -> LakeCatResult<QueryGraphTenantProjection> {
    let warehouse_record = match state.store.load_warehouse(&state.warehouse).await {
        Ok(record) => Some(record),
        Err(LakeCatError::NotFound {
            object: "warehouse",
            ..
        }) => None,
        Err(err) => return Err(err),
    };
    let projects = state.store.list_projects().await?;
    let project_record = warehouse_record.as_ref().and_then(|warehouse| {
        projects
            .iter()
            .find(|project| project.project_id == warehouse.project_id)
            .cloned()
    });
    let servers = state.store.list_servers().await?;
    let server_record = project_record
        .as_ref()
        .and_then(|project| project.server_id.as_ref())
        .and_then(|server_id| {
            servers
                .iter()
                .find(|server| server.server_id == *server_id)
                .cloned()
        });
    Ok(QueryGraphTenantProjection::from_records(
        &state.warehouse,
        warehouse_record.as_ref(),
        project_record.as_ref(),
        server_record.as_ref(),
    ))
}

async fn querygraph_view_version_receipts(
    state: &LakeCatState,
    views: &[ViewRecord],
) -> LakeCatResult<Vec<QueryGraphViewReceiptEvidence>> {
    let mut receipts = Vec::new();
    for view in views {
        let version_receipts = state
            .store
            .list_view_version_receipts(&view.warehouse, &view.namespace, &view.name)
            .await?;
        let response_receipts = version_receipts
            .iter()
            .map(view_version_receipt_response)
            .collect::<LakeCatResult<Vec<_>>>()?;
        if !view_version_receipt_chain_verified(&response_receipts) {
            return Err(LakeCatError::Internal(format!(
                "querygraph bootstrap view {}.{} has an unverified receipt chain",
                view.namespace.path(),
                view.name.as_str()
            )));
        }
        let receipt_chain_hash = view_version_receipt_chain_hash(&response_receipts)?;
        if let Some(receipt) = response_receipts
            .iter()
            .rev()
            .find(|receipt| receipt.view_version == view.view_version)
        {
            receipts.push(QueryGraphViewReceiptEvidence {
                stable_id: receipt.stable_id.clone(),
                view_version: receipt.view_version,
                receipt_hash: receipt.receipt_hash.clone(),
                receipt_chain_hash,
            });
        } else {
            return Err(LakeCatError::Internal(format!(
                "querygraph bootstrap view {}.{} is missing receipt evidence for version {}",
                view.namespace.path(),
                view.name.as_str(),
                view.view_version
            )));
        }
    }
    Ok(receipts)
}

async fn drain_lineage_outbox(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<LineageDrainResponse>, LakeCatHttpError> {
    let capability = authorize_lineage_read(&state, request_identity(&headers)?).await?;
    let mut response = drain_outbox_once(&state, 100).await?;
    attach_lineage_drain_authorization(&mut response, capability.receipt())?;
    Ok(Json(response))
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
    } else if event.event_type == "catalog.config-read" {
        let warehouse = outbox_warehouse(event, &state.warehouse)?;
        state
            .graph
            .emit(
                GraphEvent::warehouse(GraphAction::Loaded, warehouse, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::CatalogConfigRead,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if matches!(
        event.event_type.as_str(),
        "namespace.created" | "namespace.dropped" | "namespace.loaded"
    ) {
        let (warehouse, namespace) = outbox_namespace(event, &state.warehouse)?;
        let (graph_action, lineage_type) = match event.event_type.as_str() {
            "namespace.dropped" => (GraphAction::Deleted, LineageEventType::NamespaceDropped),
            "namespace.loaded" => (GraphAction::Loaded, LineageEventType::NamespaceLoaded),
            _ => (GraphAction::Created, LineageEventType::NamespaceCreated),
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
    } else if event.event_type == "namespace.listed" {
        let warehouse = outbox_warehouse(event, &state.warehouse)?;
        state
            .graph
            .emit(
                GraphEvent::warehouse(GraphAction::Loaded, warehouse, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::NamespaceListed,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if matches!(
        event.event_type.as_str(),
        "policy-binding.listed"
            | "project.listed"
            | "server.listed"
            | "storage-profile.listed"
            | "warehouse.listed"
    ) {
        let lineage_type = match event.event_type.as_str() {
            "policy-binding.listed" => LineageEventType::PolicyBindingListed,
            "project.listed" => LineageEventType::ProjectListed,
            "server.listed" => LineageEventType::ServerListed,
            "storage-profile.listed" => LineageEventType::StorageProfileListed,
            _ => LineageEventType::WarehouseListed,
        };
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
    } else if event.event_type == "view.listed" {
        let (warehouse, namespace) = outbox_namespace(event, &state.warehouse)?;
        state
            .graph
            .emit(
                GraphEvent::namespace(
                    GraphAction::Loaded,
                    warehouse,
                    namespace,
                    event_payload.clone(),
                )
                .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ViewListed,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "view.version-receipts-listed" {
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ViewVersionReceiptsListed,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "table.commits-listed" {
        if let Some(table) = table.clone() {
            project_table_commit_history_graph_events(
                state,
                event,
                &table,
                &event_payload,
                &mut receipt,
            )
            .await?;
        }
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::TableCommitRecordsListed,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "view.version-receipt-chains-listed" {
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ViewVersionReceiptChainsListed,
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
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::PolicyBindingUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
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
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ProjectUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "server.upserted" {
        let server_id = outbox_server(event)?;
        state
            .graph
            .emit(
                GraphEvent::server(GraphAction::Upserted, server_id, event_payload.clone())
                    .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::ServerUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "storage-profile.upserted" {
        let (warehouse, profile_id) = outbox_storage_profile(event, &state.warehouse)?;
        let event_payload = redact_storage_profile_event_payload(event_payload);
        state
            .graph
            .emit(
                GraphEvent::storage_profile(
                    GraphAction::Upserted,
                    warehouse,
                    profile_id,
                    event_payload.clone(),
                )
                .with_event_id(event.event_id.clone()),
            )
            .await?;
        receipt.graph_events += 1;
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::StorageProfileUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if matches!(
        event.event_type.as_str(),
        "view.upserted" | "view.loaded" | "view.dropped"
    ) {
        let (warehouse, namespace, view_name) = outbox_view(event, &state.warehouse)?;
        let (graph_action, lineage_type) = match event.event_type.as_str() {
            "view.dropped" => (GraphAction::Deleted, LineageEventType::ViewDropped),
            "view.loaded" => (GraphAction::Loaded, LineageEventType::ViewLoaded),
            _ => (GraphAction::Upserted, LineageEventType::ViewUpserted),
        };
        state
            .graph
            .emit(
                GraphEvent::view(
                    graph_action,
                    warehouse,
                    namespace,
                    view_name.as_str(),
                    event_payload.clone(),
                )
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
        let lineage_receipt = state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::WarehouseUpserted,
                principal,
                None,
                event_payload,
            ))
            .await?;
        receipt.record_lineage(lineage_receipt);
    } else if event.event_type == "table.restored" {
        if let Some(table) = table {
            state
                .graph
                .emit(
                    GraphEvent::table(GraphAction::Loaded, table.clone(), event_payload.clone())
                        .with_event_id(event.event_id.clone()),
                )
                .await?;
            receipt.graph_events += 1;
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
        if let Some((warehouse, profile_id)) =
            outbox_optional_storage_profile(event, &state.warehouse)?
        {
            let credential_payload = redact_storage_profile_event_payload(event_payload.clone());
            state
                .graph
                .emit(
                    GraphEvent::storage_profile(
                        GraphAction::Loaded,
                        warehouse,
                        profile_id,
                        credential_payload,
                    )
                    .with_event_id(format!("{}:storage-profile", event.event_id)),
                )
                .await?;
            receipt.graph_events += 1;
        }
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

async fn project_table_commit_history_graph_events(
    state: &LakeCatState,
    event: &OutboxEvent,
    table: &TableIdent,
    event_payload: &Value,
    receipt: &mut OutboxProjectionReceipt,
) -> Result<(), LakeCatError> {
    for (sequence_number, commit_hash) in outbox_commit_history_entries(event)? {
        state
            .graph
            .emit(
                GraphEvent::commit(
                    GraphAction::Loaded,
                    table,
                    sequence_number,
                    json!({
                        "event-type": event.event_type,
                        "table": table,
                        "sequence-number": sequence_number,
                        "commit-hash": commit_hash,
                        "commit-history-read": event_payload,
                    }),
                )
                .with_event_id(format!(
                    "{}:commit-history:{sequence_number}",
                    event.event_id
                )),
            )
            .await?;
        receipt.graph_events += 1;
    }
    Ok(())
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

fn outbox_server(event: &OutboxEvent) -> Result<String, LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    payload
        .get("server-id")
        .or_else(|| {
            payload
                .get("server-record")
                .and_then(|record| record.get("server-id"))
        })
        .and_then(Value::as_str)
        .filter(|server_id| !server_id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} is missing server payload",
                event.event_id
            ))
        })
}

fn outbox_storage_profile(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<(WarehouseName, String), LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let storage_profile = payload.get("storage-profile").ok_or_else(|| {
        LakeCatError::Internal(format!(
            "outbox event {} is missing storage profile payload",
            event.event_id
        ))
    })?;
    let warehouse = storage_profile
        .get("warehouse")
        .or_else(|| payload.get("warehouse"))
        .and_then(Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    let profile_id = storage_profile
        .get("profile-id")
        .and_then(Value::as_str)
        .filter(|profile_id| !profile_id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} is missing storage profile payload",
                event.event_id
            ))
        })?;
    Ok((warehouse, profile_id))
}

fn outbox_optional_storage_profile(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<Option<(WarehouseName, String)>, LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let Some(profile_id) = payload
        .get("storage-profile")
        .and_then(|storage_profile| storage_profile.get("profile-id"))
        .or_else(|| payload.get("storage-profile-id"))
        .and_then(Value::as_str)
        .filter(|profile_id| !profile_id.is_empty())
    else {
        return Ok(None);
    };
    let warehouse = payload
        .get("storage-profile")
        .and_then(|storage_profile| storage_profile.get("warehouse"))
        .or_else(|| payload.get("warehouse"))
        .or_else(|| {
            payload
                .get("table")
                .and_then(|table| table.get("warehouse"))
        })
        .and_then(Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    Ok(Some((warehouse, profile_id.to_string())))
}

fn outbox_view(
    event: &OutboxEvent,
    default_warehouse: &WarehouseName,
) -> Result<(WarehouseName, Namespace, String), LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let view = payload.get("view").ok_or_else(|| {
        LakeCatError::Internal(format!(
            "outbox event {} is missing view payload",
            event.event_id
        ))
    })?;
    let warehouse = view
        .get("warehouse")
        .or_else(|| payload.get("warehouse"))
        .and_then(Value::as_str)
        .map(WarehouseName::new)
        .transpose()?
        .unwrap_or_else(|| default_warehouse.clone());
    let namespace_value = view
        .get("namespace")
        .or_else(|| payload.get("namespace"))
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} view payload is missing namespace",
                event.event_id
            ))
        })?;
    let namespace = match namespace_value {
        Value::Array(parts) => Namespace::new(
            parts
                .iter()
                .map(|part| {
                    part.as_str().map(ToString::to_string).ok_or_else(|| {
                        LakeCatError::Internal(format!(
                            "outbox event {} view namespace components must be strings",
                            event.event_id
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?,
        Value::String(path) => path.parse::<Namespace>()?,
        _ => {
            return Err(LakeCatError::Internal(format!(
                "outbox event {} view namespace must be an array or string",
                event.event_id
            )));
        }
    };
    let view_name = view
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} view payload is missing name",
                event.event_id
            ))
        })?
        .to_string();
    Ok((warehouse, namespace, view_name))
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

fn outbox_commit_history_entries(
    event: &OutboxEvent,
) -> Result<Vec<(u64, Option<String>)>, LakeCatError> {
    let payload = event.payload.get("payload").unwrap_or(&event.payload);
    let sequence_numbers = payload
        .get("sequence-numbers")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            LakeCatError::Internal(format!(
                "outbox event {} commit history payload is missing sequence numbers",
                event.event_id
            ))
        })?;
    let commit_hashes = payload
        .get("commit-hashes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    sequence_numbers
        .iter()
        .enumerate()
        .map(|(index, sequence_number)| {
            let sequence_number = sequence_number.as_u64().ok_or_else(|| {
                LakeCatError::Internal(format!(
                    "outbox event {} commit history payload has a non-numeric sequence number",
                    event.event_id
                ))
            })?;
            let commit_hash = commit_hashes
                .get(index)
                .and_then(Value::as_str)
                .map(ToString::to_string);
            Ok((sequence_number, commit_hash))
        })
        .collect()
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

fn table_commit_record_response(
    record: &TableCommitRecord,
) -> LakeCatResult<TableCommitRecordResponse> {
    Ok(TableCommitRecordResponse {
        warehouse: record.table.warehouse.as_str().to_string(),
        namespace: record.table.namespace.parts().to_vec(),
        table: record.table.name.as_str().to_string(),
        previous_metadata_location: record.previous_metadata_location.clone(),
        new_metadata_location: record.new_metadata_location.clone(),
        sequence_number: record.sequence_number,
        format_version: record.format_version,
        snapshot_id: record.snapshot_id,
        policy_hash: record.policy_hash.clone(),
        request_hash: record.request_hash.clone(),
        response_hash: record.response_hash.clone(),
        idempotency_key_sha256: record.idempotency_key_sha256.clone(),
        commit_hash: content_hash_json(&serde_json::to_value(record).map_err(|err| {
            LakeCatError::Internal(format!("failed to serialize table commit record: {err}"))
        })?)?,
        principal_subject: record.principal.subject.clone(),
        principal_kind: principal_kind_name(&record.principal.kind).to_string(),
        committed_at: record.committed_at.to_rfc3339(),
    })
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

fn apply_credential_ttl_cap(
    mut credentials: Vec<StorageCredential>,
    max_credential_ttl_seconds: Option<u64>,
) -> Vec<StorageCredential> {
    let Some(max_credential_ttl_seconds) = max_credential_ttl_seconds else {
        return credentials;
    };
    for credential in &mut credentials {
        let mut effective = max_credential_ttl_seconds;
        credential.config.retain(|entry| {
            if entry.key == "lakecat.max-credential-ttl-seconds" {
                if let Ok(existing) = entry.value.parse::<u64>() {
                    effective = effective.min(existing);
                }
                false
            } else {
                true
            }
        });
        credential.config.push(ConfigEntry::new(
            "lakecat.max-credential-ttl-seconds",
            effective.to_string(),
        ));
    }
    credentials
}

fn canonicalize_credential_response_evidence(
    mut credentials: Vec<StorageCredential>,
    profile: &StorageProfile,
    receipt: &AuthorizationReceipt,
    governed_read_required: bool,
    max_credential_ttl_seconds: Option<u64>,
) -> Vec<StorageCredential> {
    for credential in &mut credentials {
        let mut effective_ttl = max_credential_ttl_seconds;
        credential.config.retain(|entry| {
            let normalized = entry.key.to_ascii_lowercase();
            if normalized == "lakecat.max-credential-ttl-seconds" {
                if let Ok(existing) = entry.value.parse::<u64>() {
                    effective_ttl =
                        Some(effective_ttl.map_or(existing, |current| current.min(existing)));
                }
                return false;
            }
            !LAKECAT_CREDENTIAL_RESPONSE_EVIDENCE_KEYS.contains(&normalized.as_str())
        });
        credential.config.extend([
            ConfigEntry::new("lakecat.storage-profile-id", profile.profile_id.clone()),
            ConfigEntry::new("lakecat.storage-provider", profile.provider.as_str()),
            ConfigEntry::new("lakecat.credential-mode", profile.issuance_mode.as_str()),
            ConfigEntry::new(
                "lakecat.authorization-principal",
                receipt.principal.subject.clone(),
            ),
            ConfigEntry::new(
                "lakecat.governed-read-required",
                governed_read_required.to_string(),
            ),
        ]);
        if let Some(effective_ttl) = effective_ttl {
            credential.config.push(ConfigEntry::new(
                "lakecat.max-credential-ttl-seconds",
                effective_ttl.to_string(),
            ));
        }
    }
    credentials
}

const LAKECAT_CREDENTIAL_RESPONSE_EVIDENCE_KEYS: &[&str] = &[
    "lakecat.storage-profile-id",
    "lakecat.storage-provider",
    "lakecat.credential-mode",
    "lakecat.authorization-principal",
    "lakecat.governed-read-required",
];

fn issued_credentials_for_profile(
    credentials: Vec<StorageCredential>,
    profile: &StorageProfile,
    max_credential_ttl_seconds: Option<u64>,
) -> LakeCatResult<Vec<StorageCredential>> {
    for credential in &credentials {
        if !location_is_within_prefix(&credential.prefix, &profile.location_prefix) {
            return Err(LakeCatError::InvalidArgument(format!(
                "issued credential prefix is outside storage profile scope; \
                 credential-prefix-hash={}; storage-profile-prefix-hash={}; \
                 storage-profile='{}'",
                content_hash_json(&json!({"credential-prefix": &credential.prefix}))?,
                content_hash_json(&json!({"location-prefix": &profile.location_prefix}))?,
                profile.profile_id
            )));
        }
    }
    Ok(apply_credential_ttl_cap(
        credentials,
        max_credential_ttl_seconds,
    ))
}

fn storage_profile_event_payload(profile: &StorageProfile) -> Value {
    let mut payload = json!({
        "profile-id": profile.profile_id.clone(),
        "warehouse": profile.warehouse.as_str(),
        "location-prefix": profile.location_prefix.clone(),
        "provider": profile.provider.as_str(),
        "issuance-mode": profile.issuance_mode.as_str(),
        "secret-ref-present": profile.secret_ref.is_some(),
        "public-config": profile.public_config.clone(),
    });
    if let Some(provider) = profile
        .secret_ref
        .as_deref()
        .and_then(secret_ref_provider_label)
    {
        payload["secret-ref-provider"] = json!(provider);
    }
    if let Some(secret_ref) = profile.secret_ref.as_deref() {
        payload["secret-ref-hash"] = json!(content_hash_bytes(secret_ref.as_bytes()));
    }
    payload
}

fn redact_storage_profile_event_payload(mut payload: Value) -> Value {
    let Some(profile) = payload
        .get_mut("storage-profile")
        .and_then(Value::as_object_mut)
    else {
        return payload;
    };
    if let Some(location_prefix) = profile.remove("location-prefix").and_then(|value| {
        value
            .as_str()
            .map(|location_prefix| location_prefix.to_string())
    }) && let Ok(hash) = content_hash_json(&json!({"location-prefix": location_prefix}))
    {
        profile.insert("location-prefix-hash".to_string(), json!(hash));
    }
    let raw_secret_ref = profile
        .remove("secret-ref")
        .and_then(|value| value.as_str().map(str::to_string));
    let provider = raw_secret_ref
        .as_deref()
        .and_then(secret_ref_provider_label)
        .map(str::to_string);
    if let Some(secret_ref) = raw_secret_ref.as_deref() {
        profile.insert(
            "secret-ref-hash".to_string(),
            json!(content_hash_bytes(secret_ref.as_bytes())),
        );
    }
    let secret_ref_present = raw_secret_ref.is_some()
        || profile
            .get("secret-ref-present")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    profile.insert("secret-ref-present".to_string(), json!(secret_ref_present));
    if let Some(provider) = provider {
        profile.insert("secret-ref-provider".to_string(), json!(provider));
    }
    payload
}

fn secret_ref_provider_label(secret_ref: &str) -> Option<&str> {
    secret_ref.split_once("://").map(|(scheme, _)| scheme)
}

fn storage_profile_response(profile: &StorageProfile) -> StorageProfileResponse {
    let secret_ref = profile.secret_ref.as_deref();
    StorageProfileResponse {
        profile_id: profile.profile_id.clone(),
        warehouse: profile.warehouse.as_str().to_string(),
        location_prefix: profile.location_prefix.clone(),
        provider: profile.provider.as_str().to_string(),
        issuance_mode: profile.issuance_mode.as_str().to_string(),
        secret_ref: None,
        secret_ref_present: secret_ref.is_some(),
        secret_ref_provider: secret_ref
            .and_then(secret_ref_provider_label)
            .map(str::to_string),
        secret_ref_hash: secret_ref.map(|secret_ref| content_hash_bytes(secret_ref.as_bytes())),
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
        view_version: record.view_version,
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

fn view_version_receipt_response(
    receipt: &ViewVersionReceipt,
) -> LakeCatResult<ViewVersionReceiptResponse> {
    Ok(ViewVersionReceiptResponse {
        stable_id: receipt.stable_id.clone(),
        warehouse: receipt.warehouse.as_str().to_string(),
        namespace: receipt.namespace.parts().to_vec(),
        name: receipt.name.as_str().to_string(),
        view_version: receipt.view_version,
        previous_view_version: receipt.previous_view_version,
        previous_receipt_hash: receipt.previous_receipt_hash.clone(),
        operation: view_version_operation(&receipt.operation).to_string(),
        view_hash: receipt.view_hash.clone(),
        receipt_hash: content_hash_json(&serde_json::to_value(receipt).map_err(|err| {
            LakeCatError::Internal(format!("failed to serialize view receipt: {err}"))
        })?)?,
        principal_subject: receipt.principal.subject.clone(),
        principal_kind: principal_kind_name(&receipt.principal.kind).to_string(),
        recorded_at: receipt.recorded_at.to_rfc3339(),
    })
}

fn view_version_receipt_chains(
    receipts: &[ViewVersionReceipt],
) -> LakeCatResult<Vec<ViewVersionReceiptChainResponse>> {
    let mut grouped = BTreeMap::<String, Vec<&ViewVersionReceipt>>::new();
    for receipt in receipts {
        grouped
            .entry(receipt.stable_id.clone())
            .or_default()
            .push(receipt);
    }

    grouped
        .into_values()
        .filter_map(|mut receipts| {
            receipts.sort_by(|left, right| {
                left.view_version
                    .cmp(&right.view_version)
                    .then_with(|| left.recorded_at.cmp(&right.recorded_at))
                    .then_with(|| {
                        view_version_operation(&left.operation)
                            .cmp(view_version_operation(&right.operation))
                    })
            });
            let latest = receipts.last().copied()?;
            let response_receipts = receipts
                .iter()
                .map(|receipt| view_version_receipt_response(receipt))
                .collect::<LakeCatResult<Vec<_>>>();
            Some(response_receipts.and_then(|response_receipts| {
                let chain_hash = view_version_receipt_chain_hash(&response_receipts)?;
                let chain_verified = view_version_receipt_chain_verified(&response_receipts);
                Ok(ViewVersionReceiptChainResponse {
                    stable_id: latest.stable_id.clone(),
                    warehouse: latest.warehouse.as_str().to_string(),
                    namespace: latest.namespace.parts().to_vec(),
                    name: latest.name.as_str().to_string(),
                    chain_hash,
                    chain_verified,
                    latest_view_version: latest.view_version,
                    latest_operation: view_version_operation(&latest.operation).to_string(),
                    tombstoned: latest.operation == ViewVersionOperation::Drop,
                    receipt_count: response_receipts.len(),
                    receipts: response_receipts,
                })
            }))
        })
        .collect()
}

fn view_version_receipt_chain_hash(
    receipts: &[ViewVersionReceiptResponse],
) -> LakeCatResult<String> {
    let latest = receipts.last().ok_or_else(|| {
        LakeCatError::InvalidArgument(
            "view receipt-chain hash requires at least one receipt".to_string(),
        )
    })?;
    content_hash_json(&json!({
        "stable-id": latest.stable_id,
        "warehouse": latest.warehouse,
        "namespace": latest.namespace,
        "name": latest.name,
        "latest-view-version": latest.view_version,
        "latest-operation": latest.operation,
        "tombstoned": latest.operation == "drop",
        "receipt-hashes": receipts
            .iter()
            .map(|receipt| receipt.receipt_hash.clone())
            .collect::<Vec<_>>(),
    }))
}

fn view_version_receipt_chain_verified(receipts: &[ViewVersionReceiptResponse]) -> bool {
    if receipts.is_empty() {
        return false;
    }

    receipts.iter().enumerate().all(|(index, receipt)| {
        if receipt.view_version == 0 {
            return false;
        }
        let Some(previous) = index.checked_sub(1).and_then(|index| receipts.get(index)) else {
            return receipt.operation == "upsert"
                && receipt.view_version == 1
                && receipt.previous_view_version.is_none()
                && receipt.previous_receipt_hash.is_none();
        };
        if receipt.previous_receipt_hash.as_deref() != Some(previous.receipt_hash.as_str()) {
            return false;
        }
        match receipt.operation.as_str() {
            "upsert" => {
                receipt.previous_view_version == Some(previous.view_version)
                    && receipt.view_version == previous.view_version.saturating_add(1)
            }
            "drop" => {
                receipt.previous_view_version == Some(previous.view_version)
                    && receipt.view_version == previous.view_version
            }
            _ => false,
        }
    })
}

fn principal_kind_name(kind: &PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Anonymous => "anonymous",
        PrincipalKind::Human => "human",
        PrincipalKind::Service => "service",
        PrincipalKind::Agent => "agent",
    }
}

fn view_version_operation(operation: &ViewVersionOperation) -> &'static str {
    match operation {
        ViewVersionOperation::Upsert => "upsert",
        ViewVersionOperation::Drop => "drop",
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
    let key_bytes = value.as_bytes();
    if key_bytes.is_empty() || key_bytes.len() > 128 || !key_bytes.iter().all(u8::is_ascii) {
        return Err(LakeCatError::InvalidArgument(
            "x-lakecat-idempotency-key must be 1..=128 ASCII characters".to_string(),
        )
        .into());
    }
    let key = std::str::from_utf8(key_bytes).map_err(|_| {
        LakeCatError::InvalidArgument(
            "x-lakecat-idempotency-key must be 1..=128 ASCII characters".to_string(),
        )
    })?;
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
    use http::{HeaderValue, Method, Request, StatusCode};
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
    struct FailingGraph {
        events: Mutex<Vec<GraphEvent>>,
    }

    #[async_trait]
    impl CatalogGraphSink for FailingGraph {
        async fn emit(&self, event: GraphEvent) -> lakecat_core::LakeCatResult<()> {
            self.events.lock().await.push(event);
            Err(LakeCatError::Internal(
                "intentional graph projection failure".to_string(),
            ))
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
    struct FailingLineage {
        events: Mutex<Vec<LineageEvent>>,
    }

    #[async_trait]
    impl LineageSink for FailingLineage {
        async fn emit(&self, event: LineageEvent) -> lakecat_core::LakeCatResult<LineageReceipt> {
            self.events.lock().await.push(event);
            Err(LakeCatError::Internal(
                "intentional lineage projection failure".to_string(),
            ))
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

    #[derive(Debug, Default)]
    struct DuplicateTtlCredentialIssuer {
        requests: Mutex<Vec<CredentialIssuanceRequest>>,
    }

    #[async_trait]
    impl CredentialIssuer for DuplicateTtlCredentialIssuer {
        async fn issue(
            &self,
            request: CredentialIssuanceRequest,
        ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
            self.requests.lock().await.push(request.clone());
            Ok(vec![StorageCredential {
                prefix: request.profile.location_prefix.clone(),
                config: vec![
                    ConfigEntry::new("lakecat.credential-kind", "duplicate-ttl-test"),
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "600"),
                    ConfigEntry::new("aws.session-token", "temporary-test-token"),
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "120"),
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "not-a-number"),
                ],
            }])
        }
    }

    #[derive(Debug, Default)]
    struct ShadowingCredentialEvidenceIssuer {
        requests: Mutex<Vec<CredentialIssuanceRequest>>,
    }

    #[async_trait]
    impl CredentialIssuer for ShadowingCredentialEvidenceIssuer {
        async fn issue(
            &self,
            request: CredentialIssuanceRequest,
        ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
            self.requests.lock().await.push(request.clone());
            Ok(vec![StorageCredential {
                prefix: request.profile.location_prefix.clone(),
                config: vec![
                    ConfigEntry::new("lakecat.storage-profile-id", "forged-profile"),
                    ConfigEntry::new("lakecat.storage-provider", "gcs"),
                    ConfigEntry::new("lakecat.credential-mode", "forged-mode"),
                    ConfigEntry::new("lakecat.authorization-principal", "did:example:attacker"),
                    ConfigEntry::new("lakecat.governed-read-required", "false"),
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "600"),
                    ConfigEntry::new("lakecat.credential-kind", "shadow-test"),
                    ConfigEntry::new("aws.session-token", "temporary-test-token"),
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "120"),
                ],
            }])
        }
    }

    #[derive(Debug, Default)]
    struct BroadCredentialIssuer {
        requests: Mutex<Vec<CredentialIssuanceRequest>>,
    }

    #[async_trait]
    impl CredentialIssuer for BroadCredentialIssuer {
        async fn issue(
            &self,
            request: CredentialIssuanceRequest,
        ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
            self.requests.lock().await.push(request.clone());
            Ok(vec![StorageCredential {
                prefix: "s3://lakecat-demo".to_string(),
                config: vec![
                    ConfigEntry::new("lakecat.credential-kind", "broad-test"),
                    ConfigEntry::new("aws.session-token", "temporary-test-token"),
                ],
            }])
        }
    }

    #[derive(Debug, Default)]
    struct RecordingSailEngine {
        commit_prepare_count: Mutex<usize>,
    }

    #[async_trait]
    impl SailCatalogEngine for RecordingSailEngine {
        async fn prepare_commit(
            &self,
            _request: lakecat_sail::CommitPreparationRequest,
        ) -> lakecat_core::LakeCatResult<lakecat_sail::CommitPlan> {
            *self.commit_prepare_count.lock().await += 1;
            Err(LakeCatError::Internal(
                "recording Sail engine should not prepare commit".to_string(),
            ))
        }

        async fn plan_scan(
            &self,
            _request: lakecat_sail::ScanPlanningRequest,
        ) -> lakecat_core::LakeCatResult<lakecat_sail::ScanPlan> {
            Err(LakeCatError::NotSupported(
                "recording Sail engine does not plan scans".to_string(),
            ))
        }

        async fn fetch_scan_tasks(
            &self,
            _request: lakecat_sail::FetchScanTasksRequest,
        ) -> lakecat_core::LakeCatResult<lakecat_sail::FetchScanTasksPlan> {
            Err(LakeCatError::NotSupported(
                "recording Sail engine does not fetch scan tasks".to_string(),
            ))
        }
    }

    #[derive(Debug, Default)]
    struct CapturingSailEngine {
        last_scan: Mutex<Option<lakecat_sail::ScanPlanningRequest>>,
        last_fetch: Mutex<Option<lakecat_sail::FetchScanTasksRequest>>,
    }

    #[async_trait]
    impl SailCatalogEngine for CapturingSailEngine {
        async fn prepare_commit(
            &self,
            _request: lakecat_sail::CommitPreparationRequest,
        ) -> lakecat_core::LakeCatResult<lakecat_sail::CommitPlan> {
            Err(LakeCatError::Internal(
                "capturing Sail engine should not prepare commit".to_string(),
            ))
        }

        async fn plan_scan(
            &self,
            request: lakecat_sail::ScanPlanningRequest,
        ) -> lakecat_core::LakeCatResult<lakecat_sail::ScanPlan> {
            *self.last_scan.lock().await = Some(request.clone());
            Ok(lakecat_sail::ScanPlan {
                planned_by: "capturing-sail".to_string(),
                snapshot_id: Some(42),
                scan_tasks: vec![serde_json::json!({
                    "plan-task": "lakecat:plan:captured"
                })],
                residual_filter: Some(serde_json::json!({
                    "projection": request.projection,
                    "filters": request.filters
                })),
            })
        }

        async fn fetch_scan_tasks(
            &self,
            request: lakecat_sail::FetchScanTasksRequest,
        ) -> lakecat_core::LakeCatResult<lakecat_sail::FetchScanTasksPlan> {
            *self.last_fetch.lock().await = Some(request.clone());
            Ok(lakecat_sail::FetchScanTasksPlan {
                planned_by: "capturing-sail".to_string(),
                plan_task: request.plan_task,
                snapshot_id: Some(42),
                file_scan_tasks: vec![
                    serde_json::json!({"file-path": "file:///tmp/events/data.parquet"}),
                ],
                delete_files: Vec::new(),
                plan_tasks: Vec::new(),
                residual_filter: Some(serde_json::json!({
                    "required-projection": request.required_projection,
                    "required-filters": request.required_filters
                })),
            })
        }
    }

    fn test_view_receipt(
        view_version: u64,
        previous_view_version: Option<u64>,
        previous_receipt_hash: Option<&str>,
        operation: &str,
        receipt_hash: &str,
    ) -> ViewVersionReceiptResponse {
        ViewVersionReceiptResponse {
            stable_id: "lakecat:view:local:default:events_view".to_string(),
            warehouse: "local".to_string(),
            namespace: vec!["default".to_string()],
            name: "events_view".to_string(),
            view_version,
            previous_view_version,
            previous_receipt_hash: previous_receipt_hash.map(str::to_string),
            operation: operation.to_string(),
            view_hash: format!("sha256:view-{view_version}-{operation}"),
            receipt_hash: receipt_hash.to_string(),
            principal_subject: "operator@example.com".to_string(),
            principal_kind: "human".to_string(),
            recorded_at: "2026-06-19T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn view_receipt_chain_verifier_requires_version_transitions() {
        let receipts = vec![
            test_view_receipt(1, None, None, "upsert", "sha256:r1"),
            test_view_receipt(2, Some(1), Some("sha256:r1"), "upsert", "sha256:r2"),
            test_view_receipt(2, Some(2), Some("sha256:r2"), "drop", "sha256:r3"),
        ];
        assert!(view_version_receipt_chain_verified(&receipts));

        let zero_version = vec![test_view_receipt(0, None, None, "upsert", "sha256:r0")];
        assert!(!view_version_receipt_chain_verified(&zero_version));

        let first_receipt_drop = vec![test_view_receipt(1, None, None, "drop", "sha256:r1")];
        assert!(!view_version_receipt_chain_verified(&first_receipt_drop));

        let first_receipt_with_previous_link = vec![test_view_receipt(
            1,
            Some(1),
            Some("sha256:previous"),
            "upsert",
            "sha256:r1",
        )];
        assert!(!view_version_receipt_chain_verified(
            &first_receipt_with_previous_link
        ));

        let skipped_version = vec![
            test_view_receipt(1, None, None, "upsert", "sha256:r1"),
            test_view_receipt(3, Some(1), Some("sha256:r1"), "upsert", "sha256:r3"),
        ];
        assert!(!view_version_receipt_chain_verified(&skipped_version));

        let tombstone_advanced_version = vec![
            test_view_receipt(1, None, None, "upsert", "sha256:r1"),
            test_view_receipt(2, Some(1), Some("sha256:r1"), "drop", "sha256:r2"),
        ];
        assert!(!view_version_receipt_chain_verified(
            &tombstone_advanced_version
        ));

        let wrong_previous_version = vec![
            test_view_receipt(1, None, None, "upsert", "sha256:r1"),
            test_view_receipt(2, Some(2), Some("sha256:r1"), "upsert", "sha256:r2"),
        ];
        assert!(!view_version_receipt_chain_verified(
            &wrong_previous_version
        ));

        let wrong_previous_receipt_hash = vec![
            test_view_receipt(1, None, None, "upsert", "sha256:r1"),
            test_view_receipt(2, Some(1), Some("sha256:other"), "upsert", "sha256:r2"),
        ];
        assert!(!view_version_receipt_chain_verified(
            &wrong_previous_receipt_hash
        ));

        let unsupported_operation = vec![
            test_view_receipt(1, None, None, "upsert", "sha256:r1"),
            test_view_receipt(2, Some(1), Some("sha256:r1"), "replace", "sha256:r2"),
        ];
        assert!(!view_version_receipt_chain_verified(&unsupported_operation));
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

    #[cfg(feature = "typesec-local")]
    #[derive(Debug)]
    struct MockProductionSecretRefResolver {
        provider_label: &'static str,
        credential_prefix: Option<&'static str>,
        requests: Mutex<Vec<(String, Option<u64>)>>,
    }

    #[cfg(feature = "typesec-local")]
    #[async_trait]
    impl crate::typesec_credential_issuer::SecretRefCredentialResolver
        for MockProductionSecretRefResolver
    {
        async fn resolve(
            &self,
            request: &CredentialIssuanceRequest,
        ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
            let secret_ref = request.profile.secret_ref.clone().ok_or_else(|| {
                LakeCatError::InvalidArgument(
                    "mock production resolver missing secret ref".to_string(),
                )
            })?;
            self.requests
                .lock()
                .await
                .push((secret_ref, request.max_credential_ttl_seconds));
            Ok(vec![StorageCredential {
                prefix: self
                    .credential_prefix
                    .unwrap_or(request.profile.location_prefix.as_str())
                    .to_string(),
                config: vec![ConfigEntry::new(
                    "lakecat.credential-kind",
                    format!("{}-short-lived", self.provider_label),
                )],
            }])
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
            if event_ids
                .iter()
                .any(|event_id| event_id == "evt-partial-ack")
            {
                return Ok(event_ids.len().saturating_sub(1));
            }
            Ok(event_ids.len())
        }
    }

    #[cfg(feature = "sail-local")]
    struct CasRaceStore {
        inner: Arc<dyn CatalogStore>,
        racing_metadata_location: String,
        raced: Mutex<bool>,
    }

    #[cfg(feature = "sail-local")]
    impl CasRaceStore {
        fn new(inner: Arc<dyn CatalogStore>, racing_metadata_location: String) -> Arc<Self> {
            Arc::new(Self {
                inner,
                racing_metadata_location,
                raced: Mutex::new(false),
            })
        }
    }

    #[cfg(feature = "sail-local")]
    #[async_trait]
    impl CatalogStore for CasRaceStore {
        async fn create_namespace(
            &self,
            warehouse: &WarehouseName,
            namespace: Namespace,
        ) -> lakecat_core::LakeCatResult<()> {
            self.inner.create_namespace(warehouse, namespace).await
        }

        async fn list_namespaces(
            &self,
            warehouse: &WarehouseName,
        ) -> lakecat_core::LakeCatResult<Vec<Namespace>> {
            self.inner.list_namespaces(warehouse).await
        }

        async fn list_tables(
            &self,
            warehouse: &WarehouseName,
        ) -> lakecat_core::LakeCatResult<Vec<TableRecord>> {
            self.inner.list_tables(warehouse).await
        }

        async fn create_table(
            &self,
            table: TableRecord,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            self.inner.create_table(table).await
        }

        async fn load_table(&self, ident: &TableIdent) -> lakecat_core::LakeCatResult<TableRecord> {
            self.inner.load_table(ident).await
        }

        async fn commit_table(
            &self,
            ident: &TableIdent,
            commit: TableCommit,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            let mut raced = self.raced.lock().await;
            if !*raced {
                *raced = true;
                self.inner
                    .commit_table(
                        ident,
                        TableCommit {
                            requirements: Vec::new(),
                            updates: Vec::new(),
                            expected_previous_metadata_location: commit
                                .expected_previous_metadata_location
                                .clone(),
                            new_metadata_location: Some(self.racing_metadata_location.clone()),
                            new_metadata: None,
                            idempotency_key: None,
                            idempotency_request_hash: None,
                            principal: commit.principal.clone(),
                            authorization_receipt: commit.authorization_receipt.clone(),
                        },
                    )
                    .await?;
            }
            drop(raced);
            self.inner.commit_table(ident, commit).await
        }

        async fn table_commit_records(
            &self,
            ident: &TableIdent,
            start_version: u64,
            end_version: Option<u64>,
        ) -> lakecat_core::LakeCatResult<Vec<TableCommitRecord>> {
            self.inner
                .table_commit_records(ident, start_version, end_version)
                .await
        }

        async fn soft_delete_table(
            &self,
            ident: &TableIdent,
            principal: Principal,
            authorization_receipt: Option<serde_json::Value>,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            self.inner
                .soft_delete_table(ident, principal, authorization_receipt)
                .await
        }

        async fn restore_table(
            &self,
            ident: &TableIdent,
            principal: Principal,
            authorization_receipt: Option<serde_json::Value>,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            self.inner
                .restore_table(ident, principal, authorization_receipt)
                .await
        }

        async fn upsert_storage_profile(
            &self,
            profile: StorageProfile,
        ) -> lakecat_core::LakeCatResult<StorageProfile> {
            self.inner.upsert_storage_profile(profile).await
        }

        async fn list_storage_profiles(
            &self,
            warehouse: &WarehouseName,
        ) -> lakecat_core::LakeCatResult<Vec<StorageProfile>> {
            self.inner.list_storage_profiles(warehouse).await
        }

        async fn storage_profile_for_table(
            &self,
            table: &TableRecord,
        ) -> lakecat_core::LakeCatResult<StorageProfile> {
            self.inner.storage_profile_for_table(table).await
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
                                "allowed-columns": ["event_id"],
                                "row-predicate": {
                                    "type": "not-eq",
                                    "term": "severity",
                                    "value": "debug"
                                },
                                "policy-hashes": ["sha256:policy"]
                            },
                            "required-projection": ["event_id"],
                            "required-filters": [{
                                "type": "not-eq",
                                "term": "severity",
                                "value": "debug"
                            }],
                            "storage-location": "file:///tmp/events",
                            "metadata-location": "file:///tmp/events/metadata/00000.json",
                            "file-scan-task-count": 1,
                            "delete-file-count": 1,
                            "child-plan-task-count": 1,
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
                            "format_version": 3,
                            "snapshot_id": 42,
                            "policy_hash": "sha256:policy",
                            "request_hash": "sha256:request",
                            "response_hash": "sha256:response",
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
                            "storage-profile-id": "events-local",
                            "storage-profile": {
                                "profile-id": "events-local",
                                "warehouse": "local",
                                "provider": "file",
                                "issuance-mode": "local-file-no-secret",
                                "secret-ref-present": false
                            },
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
                                    "source": "x-lakecat-typedid-envelope",
                                    "typedid-envelope-sha256": "sha256:typedid-envelope",
                                    "typedid-proof-sha256": "sha256:typedid-proof",
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
        assert_eq!(drain.graph_events, 20);
        assert_eq!(drain.lineage_events, 8);
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
        assert_eq!(credential_summary.graph_events, 2);
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
            None
        );
        assert_eq!(
            credential_summary.replay_event_hashes,
            vec!["recorded".to_string()]
        );
        assert_eq!(
            credential_summary.replay_open_lineage_hashes,
            vec!["recorded-openlineage".to_string()]
        );
        let graph_events = graph.events.lock().await;
        let credential_profile_event = graph_events
            .iter()
            .find(|event| event.event_id.as_deref() == Some("evt-credentials:storage-profile"))
            .expect("credential replay should project storage profile graph anchor");
        assert_eq!(
            credential_profile_event.label,
            GraphNodeLabel::StorageProfile
        );
        assert_eq!(credential_profile_event.action, GraphAction::Loaded);
        assert_eq!(
            credential_profile_event.subject,
            "lakecat:warehouse:local:storage-profile:events-local"
        );
        assert_eq!(
            credential_profile_event.properties["storage-profile"]["secret-ref-present"],
            serde_json::json!(false)
        );
        drop(graph_events);
        let scan_fetch_summary = drain
            .events
            .iter()
            .find(|event| event.event_type == "table.scan-tasks-fetched")
            .expect("scan task fetch replay summary should be exposed");
        assert_eq!(scan_fetch_summary.file_scan_task_count, Some(1));
        assert_eq!(scan_fetch_summary.delete_file_count, Some(1));
        assert_eq!(scan_fetch_summary.child_plan_task_count, Some(1));
        assert_eq!(scan_fetch_summary.scan_task_count, None);
        assert_eq!(
            scan_fetch_summary.read_restriction.as_ref().unwrap()["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            scan_fetch_summary.read_restriction.as_ref().unwrap()["row-predicate"],
            serde_json::json!({
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            })
        );
        assert_eq!(
            scan_fetch_summary.required_projection,
            vec!["event_id".to_string()]
        );
        assert_eq!(
            scan_fetch_summary.required_filters,
            vec![serde_json::json!({
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            })]
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
        assert_eq!(graph_events.len(), 20);
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
        assert_eq!(
            graph_events[14].properties["commit"]["response_hash"],
            serde_json::json!("sha256:response")
        );
        assert_eq!(
            graph_events[14].properties["commit"]["format_version"],
            serde_json::json!(3)
        );
        assert_eq!(
            graph_events[14].properties["commit"]["snapshot_id"],
            serde_json::json!(42)
        );
        assert_eq!(
            graph_events[14].properties["commit"]["policy_hash"],
            serde_json::json!("sha256:policy")
        );
        assert!(
            graph_events
                .iter()
                .any(|event| event.label == GraphNodeLabel::Principal
                    && event.event_id.as_deref() == Some("evt-credentials:principal"))
        );
        let credential_profile_event = graph_events
            .iter()
            .find(|event| event.event_id.as_deref() == Some("evt-credentials:storage-profile"))
            .expect("credential replay should project storage profile graph anchor");
        assert_eq!(
            credential_profile_event.label,
            GraphNodeLabel::StorageProfile
        );
        assert_eq!(credential_profile_event.action, GraphAction::Loaded);
        assert_eq!(
            credential_profile_event.subject,
            "lakecat:warehouse:local:storage-profile:events-local"
        );
        assert!(
            graph_events
                .iter()
                .any(|event| event.label == GraphNodeLabel::Principal
                    && event.event_id.as_deref() == Some("evt-3:principal"))
        );
        assert!(
            graph_events
                .iter()
                .any(|event| event.label == GraphNodeLabel::Principal
                    && event.event_id.as_deref() == Some("evt-namespace-drop:principal"))
        );
        let namespace_drop_event = graph_events
            .iter()
            .find(|event| event.event_id.as_deref() == Some("evt-namespace-drop"))
            .expect("namespace drop should project a graph event");
        assert_eq!(namespace_drop_event.label, GraphNodeLabel::Namespace);
        assert_eq!(namespace_drop_event.action, GraphAction::Deleted);
        assert_eq!(
            namespace_drop_event.subject,
            "lakecat:warehouse:local:namespace:archived"
        );
        let policy_summary = drain
            .events
            .iter()
            .find(|event| event.event_type == "policy-binding.upserted")
            .expect("policy replay summary should be exposed");
        assert_eq!(policy_summary.graph_events, 2);
        assert_eq!(policy_summary.lineage_events, 1);
        assert_eq!(
            policy_summary.replay_event_hashes,
            vec!["recorded".to_string()]
        );
        assert_eq!(
            policy_summary.replay_open_lineage_hashes,
            vec!["recorded-openlineage".to_string()]
        );

        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 8);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::NamespaceCreated
        );
        assert_eq!(
            lineage_events[1].event_type,
            LineageEventType::PolicyBindingUpserted
        );
        assert_eq!(
            lineage_events[1].payload["policy"]["odrl"]["uid"],
            serde_json::json!("policy:agent-read")
        );
        assert_eq!(lineage_events[2].event_type, LineageEventType::TableCreated);
        assert_eq!(
            lineage_events[3].event_type,
            LineageEventType::TableScanPlanned
        );
        assert_eq!(
            lineage_events[3].payload["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            lineage_events[4].event_type,
            LineageEventType::TableCommitted
        );
        assert_eq!(
            lineage_events[4].payload["commit"]["new_metadata_location"],
            serde_json::json!("file:///tmp/events/metadata/00001.json")
        );
        assert_eq!(
            lineage_events[4].payload["commit"]["response_hash"],
            serde_json::json!("sha256:response")
        );
        assert_eq!(
            lineage_events[4].payload["commit"]["format_version"],
            serde_json::json!(3)
        );
        assert_eq!(
            lineage_events[4].payload["commit"]["snapshot_id"],
            serde_json::json!(42)
        );
        assert_eq!(
            lineage_events[4].payload["commit"]["policy_hash"],
            serde_json::json!("sha256:policy")
        );
        assert_eq!(
            lineage_events[5].event_type,
            LineageEventType::CredentialsVendAttempted
        );
        assert_eq!(
            lineage_events[5].payload["credential-count"],
            serde_json::json!(0)
        );
        assert_eq!(
            lineage_events[5].payload["lakecat:raw-credential-exception"]["allowed"],
            serde_json::json!(false)
        );
        assert_eq!(
            lineage_events[6].event_type,
            LineageEventType::QueryGraphBootstrap
        );
        assert_eq!(
            lineage_events[6].payload["authorization-receipt"]["request-identity"]["attestation-state"],
            serde_json::json!("verified")
        );
        assert_eq!(
            lineage_events[6].payload["bundle-hash"],
            serde_json::json!("sha256:bundle")
        );
        assert_eq!(
            lineage_events[6].payload["graph-hash"],
            serde_json::json!("sha256:graph")
        );
        assert_eq!(
            lineage_events[6].payload["open-lineage-hash"],
            serde_json::json!("sha256:openlineage")
        );
        assert_eq!(
            lineage_events[6].payload["querygraph-import-hash"],
            serde_json::json!("sha256:querygraph-import")
        );
        assert_eq!(
            lineage_events[6].payload["table-artifacts"][0]["cdif-hash"],
            serde_json::json!("sha256:cdif")
        );
        assert_eq!(
            lineage_events[6].payload["view-artifacts"][0]["stable-id"],
            serde_json::json!("lakecat:view:local:default:active_customers")
        );
        assert_eq!(
            lineage_events[7].event_type,
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
    async fn outbox_drain_does_not_acknowledge_projection_failures() {
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
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-lineage-fails".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.created".to_string(),
                payload: json!({
                    "audit-event-id": "audit-lineage-fails",
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
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            }]),
            delivered: Mutex::default(),
        });
        let graph = Arc::new(RecordingGraph::default());
        let lineage = Arc::new(FailingLineage::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_integrations(
                default_sail_engine(),
                AllowAllGovernanceEngine::new(),
                graph.clone(),
                lineage.clone(),
            );

        let err = drain_outbox_once(&state, 10)
            .await
            .expect_err("lineage projection failure must fail the drain");
        assert!(err.to_string().contains("lineage projection failure"));
        assert!(
            store.delivered.lock().await.is_empty(),
            "failed projection must leave the event pending for retry"
        );
        assert_eq!(lineage.events.lock().await.len(), 1);
        assert!(
            !graph.events.lock().await.is_empty(),
            "graph projection may already be emitted, so retryability depends on outbox ack"
        );
    }

    #[tokio::test]
    async fn outbox_drain_does_not_acknowledge_graph_projection_failures() {
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
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-graph-fails".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.created".to_string(),
                payload: json!({
                    "audit-event-id": "audit-graph-fails",
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
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            }]),
            delivered: Mutex::default(),
        });
        let graph = Arc::new(FailingGraph::default());
        let lineage = Arc::new(RecordingLineage::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_integrations(
                default_sail_engine(),
                AllowAllGovernanceEngine::new(),
                graph.clone(),
                lineage.clone(),
            );

        let err = drain_outbox_once(&state, 10)
            .await
            .expect_err("graph projection failure must fail the drain");
        assert!(err.to_string().contains("graph projection failure"));
        assert!(
            store.delivered.lock().await.is_empty(),
            "failed graph projection must leave the event pending for retry"
        );
        assert_eq!(graph.events.lock().await.len(), 1);
        assert!(
            lineage.events.lock().await.is_empty(),
            "lineage projection must not run after graph projection failure"
        );
    }

    #[tokio::test]
    async fn outbox_drain_rejects_partial_acknowledgement() {
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
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-partial-ack".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.created".to_string(),
                payload: json!({
                    "audit-event-id": "audit-partial-ack",
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
                lineage.clone(),
            );

        let err = drain_outbox_once(&state, 10)
            .await
            .expect_err("partial acknowledgement must fail the drain");

        let message = err.to_string();
        assert!(message.contains("outbox drain acknowledgement mismatch"));
        assert!(message.contains("projected 1 event(s)"));
        assert!(message.contains("marked 0 delivered"));
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &["evt-partial-ack".to_string()]
        );
        assert!(
            !graph.events.lock().await.is_empty(),
            "graph projection should have happened before the short acknowledgement"
        );
        assert!(
            !lineage.events.lock().await.is_empty(),
            "lineage projection should have happened before the short acknowledgement"
        );
    }

    #[tokio::test]
    async fn outbox_drain_rejects_duplicate_pending_event_ids_before_projection() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal {
            subject: "agent:writer".to_string(),
            kind: PrincipalKind::Agent,
        };
        let payload = json!({
            "audit-event-id": "audit-duplicate-id",
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
            }
        });
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![
                OutboxEvent {
                    event_id: "evt-duplicate".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "table.created".to_string(),
                    payload: payload.clone(),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-duplicate".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "table.created".to_string(),
                    payload,
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

        let err = drain_outbox_once(&state, 10)
            .await
            .expect_err("duplicate pending outbox event ids must fail before projection");

        let message = err.to_string();
        assert!(message.contains("outbox pending batch contained duplicate event id hash"));
        assert!(message.contains("sha256:"));
        assert!(
            !message.contains("evt-duplicate"),
            "duplicate event id should be redacted from the operator-facing error"
        );
        assert!(
            store.delivered.lock().await.is_empty(),
            "duplicate pending ids must fail before acknowledgement"
        );
        assert!(
            graph.events.lock().await.is_empty(),
            "duplicate pending ids must fail before graph projection"
        );
        assert!(
            lineage.events.lock().await.is_empty(),
            "duplicate pending ids must fail before lineage projection"
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_catalog_config_reads_to_graph_and_lineage() {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-config-read".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "catalog.config-read".to_string(),
                payload: json!({
                    "audit-event-id": "audit-config-read",
                    "event-type": "catalog.config-read",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "catalog-config",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "warehouse": "local",
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
                lineage.clone(),
            );

        let drain = drain_outbox_once(&state, 10).await.unwrap();
        assert_eq!(drain.delivered, 1);
        assert_eq!(drain.event_types, vec!["catalog.config-read".to_string()]);
        assert_eq!(drain.graph_events, 2);
        assert_eq!(drain.lineage_events, 1);
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &["evt-config-read".to_string()]
        );
        assert_eq!(drain.events.len(), 1);
        assert_eq!(drain.events[0].graph_events, 2);
        assert_eq!(drain.events[0].lineage_events, 1);

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 2);
        assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
        assert_eq!(
            graph_events[0].event_id.as_deref(),
            Some("evt-config-read:principal")
        );
        assert_eq!(graph_events[1].label, GraphNodeLabel::Warehouse);
        assert_eq!(graph_events[1].action, GraphAction::Loaded);
        assert_eq!(graph_events[1].subject, "lakecat:warehouse:local");
        assert_eq!(graph_events[1].event_id.as_deref(), Some("evt-config-read"));
        assert_eq!(
            graph_events[1].properties["authorization-receipt"]["principal"]["subject"],
            serde_json::json!("agent:reader")
        );
        drop(graph_events);

        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 1);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::CatalogConfigRead
        );
        assert_eq!(
            lineage_events[0].payload["warehouse"],
            serde_json::json!("local")
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_table_restores_to_graph_and_lineage() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-table-restore".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.restored".to_string(),
                payload: json!({
                    "audit-event-id": "audit-table-restore",
                    "event-type": "table.restored",
                    "table": table,
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "table-restore",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "warehouse": "local",
                        "namespace": ["default"],
                        "table": "events",
                        "metadata-location": "file:///tmp/events/metadata/00000.json",
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
                lineage.clone(),
            );

        let drain = drain_outbox_once(&state, 10).await.unwrap();
        assert_eq!(drain.delivered, 1);
        assert_eq!(drain.event_types, vec!["table.restored".to_string()]);
        assert_eq!(drain.graph_events, 2);
        assert_eq!(drain.lineage_events, 1);
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &["evt-table-restore".to_string()]
        );
        assert_eq!(drain.events.len(), 1);
        assert_eq!(drain.events[0].graph_events, 2);
        assert_eq!(drain.events[0].lineage_events, 1);

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 2);
        assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
        assert_eq!(
            graph_events[0].event_id.as_deref(),
            Some("evt-table-restore:principal")
        );
        assert_eq!(graph_events[1].label, GraphNodeLabel::Table);
        assert_eq!(graph_events[1].action, GraphAction::Loaded);
        assert_eq!(
            graph_events[1].subject,
            "lakecat:table:local:default:events"
        );
        assert_eq!(
            graph_events[1].event_id.as_deref(),
            Some("evt-table-restore")
        );
        assert_eq!(
            graph_events[1].properties["metadata-location"],
            serde_json::json!("file:///tmp/events/metadata/00000.json")
        );
        drop(graph_events);

        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 1);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::TableRestored
        );
        assert_eq!(
            lineage_events[0].payload["metadata-location"],
            serde_json::json!("file:///tmp/events/metadata/00000.json")
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_management_list_reads_to_lineage() {
        let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
        let authorization_receipt = json!({
            "principal": principal,
            "action": "management-list",
            "allowed": true,
            "engine": "test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now(),
        });
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![
                OutboxEvent {
                    event_id: "evt-policy-list".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "policy-binding.listed".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-policy-list",
                        "event-type": "policy-binding.listed",
                        "payload": {
                            "authorization-receipt": authorization_receipt,
                            "warehouse": "local",
                            "policy-count": 2,
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-project-list".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "project.listed".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-project-list",
                        "event-type": "project.listed",
                        "payload": {
                            "authorization-receipt": authorization_receipt,
                            "project-count": 1,
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-server-list".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "server.listed".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-server-list",
                        "event-type": "server.listed",
                        "payload": {
                            "authorization-receipt": authorization_receipt,
                            "server-count": 1,
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-storage-profile-list".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "storage-profile.listed".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-storage-profile-list",
                        "event-type": "storage-profile.listed",
                        "payload": {
                            "authorization-receipt": authorization_receipt,
                            "warehouse": "local",
                            "storage-profile-count": 2,
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-warehouse-list".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "warehouse.listed".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-warehouse-list",
                        "event-type": "warehouse.listed",
                        "payload": {
                            "authorization-receipt": authorization_receipt,
                            "project-id": "analytics",
                            "warehouse-count": 3,
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
        assert_eq!(drain.delivered, 5);
        assert_eq!(
            drain.event_types,
            vec![
                "policy-binding.listed".to_string(),
                "project.listed".to_string(),
                "server.listed".to_string(),
                "storage-profile.listed".to_string(),
                "warehouse.listed".to_string(),
            ]
        );
        assert_eq!(drain.graph_events, 5);
        assert_eq!(drain.lineage_events, 5);
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &[
                "evt-policy-list".to_string(),
                "evt-project-list".to_string(),
                "evt-server-list".to_string(),
                "evt-storage-profile-list".to_string(),
                "evt-warehouse-list".to_string(),
            ]
        );
        assert_eq!(drain.events.len(), 5);
        assert_eq!(drain.events[0].policy_binding_count, 2);
        assert_eq!(
            drain.events[0].management_scope_warehouse.as_deref(),
            Some("local")
        );
        assert_eq!(drain.events[1].project_count, Some(1));
        assert_eq!(drain.events[2].server_count, Some(1));
        assert_eq!(drain.events[3].storage_profile_count, Some(2));
        assert_eq!(
            drain.events[3].management_scope_warehouse.as_deref(),
            Some("local")
        );
        assert_eq!(drain.events[4].warehouse_count, Some(3));
        assert_eq!(
            drain.events[4].management_scope_project_id.as_deref(),
            Some("analytics")
        );

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 5);
        assert!(
            graph_events
                .iter()
                .all(|event| event.label == GraphNodeLabel::Principal)
        );
        drop(graph_events);

        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 5);
        let lineage_types: Vec<_> = lineage_events
            .iter()
            .map(|event| event.event_type.clone())
            .collect();
        assert_eq!(
            lineage_types,
            vec![
                LineageEventType::PolicyBindingListed,
                LineageEventType::ProjectListed,
                LineageEventType::ServerListed,
                LineageEventType::StorageProfileListed,
                LineageEventType::WarehouseListed,
            ]
        );
        assert_eq!(
            lineage_events[0].payload["policy-count"],
            serde_json::json!(2)
        );
        assert_eq!(
            lineage_events[3].payload["storage-profile-count"],
            serde_json::json!(2)
        );
        assert_eq!(
            lineage_events[4].payload["project-id"],
            serde_json::json!("analytics")
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_namespace_reads_to_graph_and_lineage() {
        let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![
                OutboxEvent {
                    event_id: "evt-namespace-list".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "namespace.listed".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-namespace-list",
                        "event-type": "namespace.listed",
                        "payload": {
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "namespace-list",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                            },
                            "warehouse": "local",
                            "namespace-count": 2,
                        }
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-namespace-load".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "namespace.loaded".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-namespace-load",
                        "event-type": "namespace.loaded",
                        "payload": {
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "namespace-load",
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
        assert_eq!(drain.delivered, 2);
        assert_eq!(
            drain.event_types,
            vec![
                "namespace.listed".to_string(),
                "namespace.loaded".to_string()
            ]
        );
        assert_eq!(drain.graph_events, 4);
        assert_eq!(drain.lineage_events, 2);
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &[
                "evt-namespace-list".to_string(),
                "evt-namespace-load".to_string()
            ]
        );
        assert_eq!(drain.events.len(), 2);
        assert_eq!(drain.events[0].graph_events, 2);
        assert_eq!(drain.events[0].lineage_events, 1);
        assert_eq!(drain.events[1].graph_events, 2);
        assert_eq!(drain.events[1].lineage_events, 1);

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 4);
        assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
        assert_eq!(
            graph_events[0].event_id.as_deref(),
            Some("evt-namespace-list:principal")
        );
        assert_eq!(graph_events[1].label, GraphNodeLabel::Warehouse);
        assert_eq!(graph_events[1].action, GraphAction::Loaded);
        assert_eq!(graph_events[1].subject, "lakecat:warehouse:local");
        assert_eq!(
            graph_events[1].event_id.as_deref(),
            Some("evt-namespace-list")
        );
        assert_eq!(graph_events[2].label, GraphNodeLabel::Principal);
        assert_eq!(
            graph_events[2].event_id.as_deref(),
            Some("evt-namespace-load:principal")
        );
        assert_eq!(graph_events[3].label, GraphNodeLabel::Namespace);
        assert_eq!(graph_events[3].action, GraphAction::Loaded);
        assert_eq!(
            graph_events[3].subject,
            "lakecat:warehouse:local:namespace:default"
        );
        assert_eq!(
            graph_events[3].event_id.as_deref(),
            Some("evt-namespace-load")
        );
        drop(graph_events);

        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 2);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::NamespaceListed
        );
        assert_eq!(
            lineage_events[0].payload["namespace-count"],
            serde_json::json!(2)
        );
        assert_eq!(
            lineage_events[1].event_type,
            LineageEventType::NamespaceLoaded
        );
        assert_eq!(
            lineage_events[1].payload["namespace"],
            serde_json::json!(["default"])
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_server_upserts_to_lineage() {
        let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-server".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "server.upserted".to_string(),
                payload: json!({
                    "audit-event-id": "audit-server",
                    "event-type": "server.upserted",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "server-manage",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "server-id": "prod",
                        "server-record": {
                            "server-id": "prod",
                            "display-name": "Production",
                            "endpoint-url": "https://lakecat.example",
                            "properties": {"region": "global"}
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
                lineage.clone(),
            );

        let drain = drain_outbox_once(&state, 10).await.unwrap();
        assert_eq!(drain.delivered, 1);
        assert_eq!(drain.event_types, vec!["server.upserted".to_string()]);
        assert_eq!(drain.graph_events, 2);
        assert_eq!(drain.lineage_events, 1);
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &["evt-server".to_string()]
        );

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 2);
        assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
        assert_eq!(graph_events[0].subject, "lakecat:principal:agent:operator");
        assert_eq!(graph_events[1].label, GraphNodeLabel::Server);
        assert_eq!(graph_events[1].subject, "lakecat:server:prod");
        assert_eq!(
            graph_events[1].properties["server-record"]["server-id"],
            serde_json::json!("prod")
        );
        drop(graph_events);

        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 1);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::ServerUpserted
        );
        assert_eq!(
            lineage_events[0].payload["server-record"]["display-name"],
            serde_json::json!("Production")
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_storage_profile_upserts_to_lineage() {
        let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-storage-profile".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "storage-profile.upserted".to_string(),
                payload: json!({
                    "audit-event-id": "audit-storage-profile",
                    "event-type": "storage-profile.upserted",
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "storage-profile-manage",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "warehouse": "local",
                        "storage-profile": {
                            "profile-id": "s3-events",
                            "warehouse": "local",
                            "location-prefix": "s3://lakecat/events",
                            "provider": "s3",
                            "issuance-mode": "secret-ref",
                            "secret-ref": "vault://kv/lakecat/events",
                            "public-config": {"region": "us-west-2"}
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
                lineage.clone(),
            );

        let drain = drain_outbox_once(&state, 10).await.unwrap();
        assert_eq!(drain.delivered, 1);
        assert_eq!(
            drain.event_types,
            vec!["storage-profile.upserted".to_string()]
        );
        assert_eq!(drain.graph_events, 2);
        assert_eq!(drain.lineage_events, 1);
        assert_eq!(
            drain.events[0].storage_profile_id.as_deref(),
            Some("s3-events")
        );
        assert_eq!(
            drain.events[0].storage_profile_provider.as_deref(),
            Some("s3")
        );
        assert_eq!(
            drain.events[0].storage_profile_issuance_mode.as_deref(),
            Some("secret-ref")
        );
        assert_eq!(
            drain.events[0]
                .storage_profile_location_prefix_hash
                .as_deref(),
            Some(
                content_hash_json(&json!({"location-prefix": "s3://lakecat/events"}))
                    .unwrap()
                    .as_str()
            )
        );
        assert_eq!(
            drain.events[0].storage_profile_secret_ref_present,
            Some(true)
        );
        assert_eq!(
            drain.events[0]
                .storage_profile_secret_ref_provider
                .as_deref(),
            Some("vault")
        );
        assert_eq!(
            drain.events[0].storage_profile_secret_ref_hash.as_deref(),
            Some(content_hash_bytes("vault://kv/lakecat/events".as_bytes()).as_str())
        );
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &["evt-storage-profile".to_string()]
        );

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 2);
        assert_eq!(graph_events[0].label, GraphNodeLabel::Principal);
        assert_eq!(graph_events[0].subject, "lakecat:principal:agent:operator");
        assert_eq!(graph_events[1].label, GraphNodeLabel::StorageProfile);
        assert_eq!(
            graph_events[1].subject,
            "lakecat:warehouse:local:storage-profile:s3-events"
        );
        assert_eq!(
            graph_events[1].properties["storage-profile"]["secret-ref-present"],
            serde_json::json!(true)
        );
        assert_eq!(
            graph_events[1].properties["storage-profile"]["secret-ref-provider"],
            serde_json::json!("vault")
        );
        assert_eq!(
            graph_events[1].properties["storage-profile"]["secret-ref-hash"],
            serde_json::json!(content_hash_bytes("vault://kv/lakecat/events".as_bytes()))
        );
        assert!(
            graph_events[1].properties["storage-profile"]
                .get("location-prefix")
                .is_none(),
            "storage profile graph projection must not expose the raw location prefix"
        );
        assert_eq!(
            graph_events[1].properties["storage-profile"]["location-prefix-hash"],
            serde_json::json!(
                content_hash_json(&json!({"location-prefix": "s3://lakecat/events"})).unwrap()
            )
        );
        assert!(
            graph_events[1].properties["storage-profile"]
                .get("secret-ref")
                .is_none(),
            "storage profile graph projection must not expose the secret-ref URI"
        );
        drop(graph_events);

        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 1);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::StorageProfileUpserted
        );
        assert_eq!(
            lineage_events[0].payload["storage-profile"]["profile-id"],
            serde_json::json!("s3-events")
        );
        assert_eq!(
            lineage_events[0].payload["storage-profile"]["provider"],
            serde_json::json!("s3")
        );
        assert_eq!(
            lineage_events[0].payload["storage-profile"]["issuance-mode"],
            serde_json::json!("secret-ref")
        );
        assert_eq!(
            lineage_events[0].payload["storage-profile"]["secret-ref-present"],
            serde_json::json!(true)
        );
        assert_eq!(
            lineage_events[0].payload["storage-profile"]["secret-ref-provider"],
            serde_json::json!("vault")
        );
        assert_eq!(
            lineage_events[0].payload["storage-profile"]["secret-ref-hash"],
            serde_json::json!(content_hash_bytes("vault://kv/lakecat/events".as_bytes()))
        );
        assert!(
            lineage_events[0].payload["storage-profile"]
                .get("location-prefix")
                .is_none(),
            "storage profile lineage projection must not expose the raw location prefix"
        );
        assert_eq!(
            lineage_events[0].payload["storage-profile"]["location-prefix-hash"],
            serde_json::json!(
                content_hash_json(&json!({"location-prefix": "s3://lakecat/events"})).unwrap()
            )
        );
        assert!(
            lineage_events[0].payload["storage-profile"]
                .get("secret-ref")
                .is_none()
        );
    }

    #[test]
    fn storage_profile_event_payload_redacts_secret_ref() {
        let profile = StorageProfile::new(
            "s3-events",
            WarehouseName::new("local").unwrap(),
            "s3://lakecat/events",
            StorageProvider::S3,
            CredentialIssuanceMode::ShortLivedSecretRef,
            Some("typesec://env/LAKECAT_S3_EVENTS".to_string()),
            BTreeMap::from([("lakecat.region".to_string(), "us-west-2".to_string())]),
        )
        .unwrap();

        let payload = storage_profile_event_payload(&profile);
        assert_eq!(payload["profile-id"], serde_json::json!("s3-events"));
        assert_eq!(payload["secret-ref-present"], serde_json::json!(true));
        assert_eq!(payload["secret-ref-provider"], serde_json::json!("typesec"));
        assert_eq!(
            payload["secret-ref-hash"],
            serde_json::json!(content_hash_bytes(
                "typesec://env/LAKECAT_S3_EVENTS".as_bytes()
            ))
        );
        assert!(payload.get("secret-ref").is_none());
    }

    #[tokio::test]
    async fn outbox_drain_projects_view_events_to_graph_and_lineage() {
        let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
        let view_payload = json!({
            "warehouse": "local",
            "namespace": ["default"],
            "view": {
                "warehouse": "local",
                "namespace": ["default"],
                "name": "events_view",
                "sql": "select event_id from default.events",
                "dialect": "spark-sql",
                "schema-version": 1,
                "view-version": 1,
                "columns": [{
                    "name": "event_id",
                    "data-type": {"type": "long"},
                    "nullable": false,
                    "comment": null
                }],
                "properties": {}
            },
            "authorization-receipt": {
                "principal": principal,
                "action": "view-manage",
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            }
        });
        let mut guarded_view_payload = view_payload.clone();
        guarded_view_payload["expected-view-version"] = json!(1);
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![
                OutboxEvent {
                    event_id: "evt-view-list".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "view.listed".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-view-list",
                        "event-type": "view.listed",
                        "payload": {
                            "warehouse": "local",
                            "namespace": ["default"],
                            "view-count": 1,
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "view-load",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                            }
                        },
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-view-upsert".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "view.upserted".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-view-upsert",
                        "event-type": "view.upserted",
                        "payload": guarded_view_payload.clone(),
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-view-load".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "view.loaded".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-view-load",
                        "event-type": "view.loaded",
                        "payload": view_payload,
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-view-drop".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "view.dropped".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-view-drop",
                        "event-type": "view.dropped",
                        "payload": guarded_view_payload.clone(),
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-view-receipts".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "view.version-receipts-listed".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-view-receipts",
                        "event-type": "view.version-receipts-listed",
                        "payload": {
                            "warehouse": "local",
                            "namespace": ["default"],
                            "view": "events_view",
                            "receipt-count": 2,
                            "receipt-hashes": ["sha256:view-upsert-receipt", "sha256:view-drop-receipt"],
                            "drop-receipt-hashes": ["sha256:view-drop-receipt"],
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "view-load",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                            }
                        },
                    }),
                    created_at: chrono::Utc::now(),
                    delivered_at: None,
                },
                OutboxEvent {
                    event_id: "evt-view-chains".to_string(),
                    sink: "lakecat.lineage-and-graph".to_string(),
                    event_type: "view.version-receipt-chains-listed".to_string(),
                    payload: json!({
                        "audit-event-id": "audit-view-chains",
                        "event-type": "view.version-receipt-chains-listed",
                        "payload": {
                            "warehouse": "local",
                            "namespace": ["default"],
                            "chain-count": 1,
                            "receipt-count": 2,
                            "tombstone-count": 1,
                            "chain-verified-count": 1,
                            "chain-hashes": ["sha256:view-receipt-chain"],
                            "receipt-hashes": ["sha256:view-upsert-receipt", "sha256:view-drop-receipt"],
                            "drop-receipt-hashes": ["sha256:view-drop-receipt"],
                            "authorization-receipt": {
                                "principal": principal,
                                "action": "view-load",
                                "allowed": true,
                                "engine": "test",
                                "policy_hash": null,
                                "checked_at": chrono::Utc::now(),
                            }
                        },
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
        assert_eq!(drain.delivered, 6);
        assert_eq!(
            drain.event_types,
            vec![
                "view.listed".to_string(),
                "view.upserted".to_string(),
                "view.loaded".to_string(),
                "view.dropped".to_string(),
                "view.version-receipts-listed".to_string(),
                "view.version-receipt-chains-listed".to_string()
            ]
        );
        assert_eq!(drain.graph_events, 10);
        assert_eq!(drain.lineage_events, 6);
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &[
                "evt-view-list".to_string(),
                "evt-view-upsert".to_string(),
                "evt-view-load".to_string(),
                "evt-view-drop".to_string(),
                "evt-view-receipts".to_string(),
                "evt-view-chains".to_string()
            ]
        );
        assert_eq!(drain.events.len(), 6);
        assert_eq!(
            drain.events[1].view_stable_id.as_deref(),
            Some("lakecat:view:local:default:events_view")
        );
        assert_eq!(drain.events[1].view_warehouse.as_deref(), Some("local"));
        assert_eq!(drain.events[1].view_namespace, vec!["default"]);
        assert_eq!(drain.events[1].view_name.as_deref(), Some("events_view"));
        assert_eq!(drain.events[1].view_version, Some(1));
        assert_eq!(drain.events[1].expected_view_version, Some(1));
        assert_eq!(drain.events[2].expected_view_version, None);
        assert_eq!(drain.events[3].expected_view_version, Some(1));
        assert_eq!(
            drain.events[4].view_stable_id.as_deref(),
            Some("lakecat:view:local:default:events_view")
        );
        assert_eq!(
            drain.events[4].view_version_receipt_hashes,
            vec!["sha256:view-drop-receipt".to_string()]
        );
        assert_eq!(
            drain.events[5].view_version_receipt_hashes,
            vec!["sha256:view-drop-receipt".to_string()]
        );
        assert_eq!(
            drain.events[5].view_version_receipt_chain_hashes,
            vec!["sha256:view-receipt-chain".to_string()]
        );
        assert_eq!(drain.events[5].view_version_receipt_chain_verified_count, 1);

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 10);
        let view_events = graph_events
            .iter()
            .filter(|event| event.label == GraphNodeLabel::View)
            .collect::<Vec<_>>();
        assert_eq!(view_events.len(), 3);
        assert_eq!(view_events[0].action, GraphAction::Upserted);
        assert_eq!(
            view_events[0].subject,
            "lakecat:warehouse:local:namespace:default:view:events_view"
        );
        assert_eq!(view_events[1].action, GraphAction::Loaded);
        assert_eq!(view_events[2].action, GraphAction::Deleted);
        assert!(graph_events.iter().any(|event| {
            event.label == GraphNodeLabel::Namespace
                && event.action == GraphAction::Loaded
                && event.event_id.as_deref() == Some("evt-view-list")
        }));
        drop(graph_events);

        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 6);
        assert_eq!(lineage_events[0].event_type, LineageEventType::ViewListed);
        assert_eq!(lineage_events[1].event_type, LineageEventType::ViewUpserted);
        assert_eq!(lineage_events[2].event_type, LineageEventType::ViewLoaded);
        assert_eq!(lineage_events[3].event_type, LineageEventType::ViewDropped);
        assert_eq!(
            lineage_events[4].event_type,
            LineageEventType::ViewVersionReceiptsListed
        );
        assert_eq!(
            lineage_events[5].event_type,
            LineageEventType::ViewVersionReceiptChainsListed
        );
        assert_eq!(
            lineage_events[1].payload["view"]["name"],
            serde_json::json!("events_view")
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
                lineage.clone(),
            );

        let drain = drain_outbox_once(&state, 10).await.unwrap();
        assert_eq!(drain.delivered, 1);
        assert_eq!(drain.event_types, vec!["warehouse.upserted".to_string()]);
        assert_eq!(drain.graph_events, 2);
        assert_eq!(drain.lineage_events, 1);
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
        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 1);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::WarehouseUpserted
        );
        assert_eq!(
            lineage_events[0].payload["warehouse-record"]["storage-root"],
            serde_json::json!("file:///tmp/lakecat")
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
                lineage.clone(),
            );

        let drain = drain_outbox_once(&state, 10).await.unwrap();
        assert_eq!(drain.delivered, 1);
        assert_eq!(drain.event_types, vec!["project.upserted".to_string()]);
        assert_eq!(drain.graph_events, 2);
        assert_eq!(drain.lineage_events, 1);
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
        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 1);
        assert_eq!(
            lineage_events[0].event_type,
            LineageEventType::ProjectUpserted
        );
        assert_eq!(
            lineage_events[0].payload["project-record"]["display-name"],
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
                            "context": {
                                "request-identity": {
                                    "attestation-state": "verified",
                                    "source": "x-lakecat-typedid-envelope",
                                    "typedid-envelope-sha256": "sha256:typedid-envelope",
                                    "typedid-proof-sha256": "sha256:typedid-proof",
                                    "agent-delegation-sha256": "sha256:delegation",
                                    "agent-summary-signature-sha256": "sha256:summary",
                                    "typedid": "did:example:agent"
                                }
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
                        "view-version-receipts": [{
                            "stable-id": "lakecat:view:local:default:active_customers",
                            "view-version": 1,
                            "receipt-hash": "sha256:view-version-receipt"
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
            payload["principal-subject"],
            serde_json::json!("did:example:agent")
        );
        assert_eq!(payload["principal-kind"], serde_json::json!("agent"));
        assert!(
            payload["authorization-receipt-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert_eq!(
            payload["request-identity-state"],
            serde_json::json!("unverified")
        );
        assert_eq!(
            payload["request-identity-source"],
            serde_json::json!("x-lakecat-agent-did")
        );
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
        assert_eq!(
            payload["events"][0]["request-identity-source"],
            serde_json::json!("x-lakecat-typedid-envelope")
        );
        assert_eq!(
            payload["events"][0]["typedid-envelope-hash"],
            serde_json::json!("sha256:typedid-envelope")
        );
        assert_eq!(
            payload["events"][0]["typedid-proof-hash"],
            serde_json::json!("sha256:typedid-proof")
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
            payload["events"][0]["view-version-receipt-hashes"],
            serde_json::json!(["sha256:view-version-receipt"])
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
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = payload["error"]["message"].as_str().unwrap();
        assert!(message.contains("idempotency key reused with different commit request"));
        assert!(!message.contains("commit:events:0001"));
        assert!(!message.contains("00001.json"));
        assert!(!message.contains("file:///tmp/events/metadata/00001.json"));

        let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
        let records = store.table_commit_records(&ident, 0, None).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].sequence_number, 1);
        assert_eq!(
            records[0].idempotency_key_sha256.as_deref(),
            Some(content_hash_bytes("commit:events:0001".as_bytes()).as_str())
        );
        let pending = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        let commit_outbox_count = pending
            .iter()
            .filter(|event| event.event_type == "table.commit")
            .count();
        assert_eq!(
            commit_outbox_count, 1,
            "idempotent replay and mismatch conflicts must not enqueue extra commit outbox events"
        );
        assert_eq!(store.load_table(&ident).await.unwrap().version, 1);
    }

    #[tokio::test]
    async fn commit_rejects_invalid_rest_idempotency_keys() {
        let sail = Arc::new(RecordingSailEngine::default());
        let governance = Arc::new(RecordingGovernance::default());
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        )
        .with_integrations(
            sail.clone(),
            governance.clone(),
            NoopCatalogGraphSink::new(),
            Arc::new(RecordingLineage::default()),
        ));
        let cases = vec![
            (
                HeaderValue::from_static("commit events 0001"),
                "x-lakecat-idempotency-key may only contain",
            ),
            (
                HeaderValue::from_str("x".repeat(129).as_str()).unwrap(),
                "x-lakecat-idempotency-key must be 1..=128 ASCII characters",
            ),
            (
                HeaderValue::from_bytes("commit:é".as_bytes()).unwrap(),
                "x-lakecat-idempotency-key must be 1..=128 ASCII characters",
            ),
            (
                HeaderValue::from_bytes(b"commit:\xff").unwrap(),
                "x-lakecat-idempotency-key must be 1..=128 ASCII characters",
            ),
        ];

        for (key, expected_message) in cases {
            let commit = Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces/default/tables/events/commit")
                .header("content-type", "application/json")
                .header("x-lakecat-idempotency-key", key)
                .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
                .unwrap();
            let response = app.clone().oneshot(commit).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let message = String::from_utf8_lossy(&body);
            assert!(message.contains(expected_message), "{message}");
        }
        assert_eq!(*sail.commit_prepare_count.lock().await, 0);
        assert!(
            governance.principals.lock().await.is_empty(),
            "invalid idempotency keys must fail before authorization"
        );
    }

    #[tokio::test]
    async fn management_table_commits_lists_pointer_log_evidence() {
        let store = MemoryCatalogStore::new();
        let graph = Arc::new(RecordingGraph::default());
        let lineage = Arc::new(RecordingLineage::default());
        let app = app(
            LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
                .with_integrations(
                    default_sail_engine(),
                    AllowAllGovernanceEngine::new(),
                    graph.clone(),
                    lineage,
                ),
        );
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-commit-history-{unique}"));
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
        let advanced_metadata = serde_json::json!({
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
                    "required": true
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
            .header("x-lakecat-principal", "operator@example.com")
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

        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .header("x-lakecat-idempotency-key", "commit:events:history")
            .body(Body::from(
                serde_json::json!({
                    "requirements": [],
                    "updates": [],
                    "metadata-location": committed_metadata_location,
                    "metadata": advanced_metadata,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses/local/namespaces/default/tables/events/commits")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let commits = body["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0]["warehouse"], serde_json::json!("local"));
        assert_eq!(commits[0]["namespace"], serde_json::json!(["default"]));
        assert_eq!(commits[0]["table"], serde_json::json!("events"));
        assert_eq!(commits[0]["sequence-number"], serde_json::json!(1));
        assert_eq!(commits[0]["format-version"], serde_json::json!(3));
        assert_eq!(commits[0]["snapshot-id"], serde_json::json!(43));
        assert!(
            commits[0]["request-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert!(
            commits[0]["response-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert!(
            commits[0]["commit-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert_eq!(
            commits[0]["idempotency-key-sha256"],
            serde_json::json!(content_hash_bytes("commit:events:history".as_bytes()))
        );
        assert_eq!(
            commits[0]["principal-subject"],
            serde_json::json!("operator@example.com")
        );

        let pending = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        let commit_read = pending
            .iter()
            .find(|event| event.event_type == "table.commits-listed")
            .expect("commit history read should enter the durable outbox");
        let commit_read_payload = &commit_read.payload["payload"];
        assert_eq!(commit_read_payload["commit-count"], serde_json::json!(1));
        assert_eq!(
            commit_read_payload["commit-hashes"][0],
            commits[0]["commit-hash"]
        );
        assert_eq!(
            commit_read_payload["sequence-numbers"],
            serde_json::json!([1])
        );

        let drain = Request::builder()
            .method(Method::POST)
            .uri("/management/v1/lineage/drain")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(drain).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            body["event-types"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event_type| event_type == "table.commits-listed")
        );
        let commit_read_summary = body["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["event-type"] == "table.commits-listed")
            .expect("drain should summarize the commit history read");
        assert_eq!(commit_read_summary["graph-events"], serde_json::json!(2));
        assert_eq!(commit_read_summary["lineage-events"], serde_json::json!(1));
        assert_eq!(
            commit_read_summary["table-commit-count"],
            serde_json::json!(1)
        );
        assert_eq!(
            commit_read_summary["table-commit-sequence-numbers"],
            serde_json::json!([1])
        );
        assert_eq!(
            commit_read_summary["table-commit-hashes"],
            serde_json::json!([commits[0]["commit-hash"].clone()])
        );
        let graph_events = graph.events.lock().await;
        assert!(
            graph_events.iter().any(|event| {
                event.label == GraphNodeLabel::Commit && event.action == GraphAction::Committed
            }),
            "drain should also project the original table.commit event"
        );
        let commit_graph_event = graph_events
            .iter()
            .find(|event| {
                event.label == GraphNodeLabel::Commit && event.action == GraphAction::Loaded
            })
            .expect("commit history read should project a loaded Commit graph event");
        assert_eq!(commit_graph_event.action, GraphAction::Loaded);
        assert_eq!(
            commit_graph_event.subject,
            "lakecat:commit:lakecat:table:local:default:events:1"
        );
        assert_eq!(
            commit_graph_event.event_id.as_deref(),
            Some(format!("{}:commit-history:1", commit_read.event_id).as_str())
        );
        assert_eq!(
            commit_graph_event.properties["commit-hash"],
            commits[0]["commit-hash"]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn idempotent_commit_replay_does_not_rewrite_metadata_object() {
        let store = MemoryCatalogStore::new();
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            store.clone(),
        ));
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-idempotent-object-{unique}"));
        let table_dir = root.join("events");
        let metadata_dir = table_dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let table_location = url::Url::from_directory_path(&table_dir)
            .expect("table dir URL")
            .to_string();
        let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
            .unwrap()
            .to_string();
        let committed_metadata_path = metadata_dir.join("00001.json");
        let committed_metadata_location = url::Url::from_file_path(&committed_metadata_path)
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
        let advanced_metadata = serde_json::json!({
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
                    "required": true
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
                    "metadata": base_metadata,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let commit_body = serde_json::json!({
            "requirements": [],
            "updates": [],
            "metadata-location": committed_metadata_location,
            "metadata": advanced_metadata,
        })
        .to_string();
        let commit = || {
            Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces/default/tables/events/commit")
                .header("content-type", "application/json")
                .header("x-lakecat-idempotency-key", "commit:events:metadata-object")
                .body(Body::from(commit_body.clone()))
                .unwrap()
        };
        let response = app.clone().oneshot(commit()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let original_written = std::fs::read_to_string(&committed_metadata_path).unwrap();
        assert!(original_written.contains("\"current-snapshot-id\": 43"));

        let sentinel = "{\n  \"sentinel\": \"replay must not rewrite metadata\"\n}\n";
        std::fs::write(&committed_metadata_path, sentinel).unwrap();
        let response = app.clone().oneshot(commit()).await.unwrap();
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
            std::fs::read_to_string(&committed_metadata_path).unwrap(),
            sentinel
        );

        let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
        let records = store.table_commit_records(&ident, 0, None).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(store.load_table(&ident).await.unwrap().version, 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn commit_rejects_metadata_object_overwrite_of_current_pointer() {
        let app = test_app();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-current-metadata-guard-{unique}"));
        let table_dir = root.join("events");
        let metadata_dir = table_dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let table_location = url::Url::from_directory_path(&table_dir)
            .expect("table dir URL")
            .to_string();
        let initial_metadata_path = metadata_dir.join("00000.json");
        let initial_metadata_location = url::Url::from_file_path(&initial_metadata_path)
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

        let sentinel = "{\n  \"sentinel\": \"current metadata must not be overwritten\"\n}\n";
        std::fs::write(&initial_metadata_path, sentinel).unwrap();
        let overwrite_metadata = serde_json::json!({
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
                    "required": true
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
        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "requirements": [],
                    "updates": [],
                    "metadata-location": initial_metadata_location,
                    "metadata": overwrite_metadata,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = payload["error"]["message"].as_str().unwrap();
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(!message.contains(&initial_metadata_location));
        assert!(!message.contains("00000.json"));
        assert_eq!(
            std::fs::read_to_string(&initial_metadata_path).unwrap(),
            sentinel
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn commit_rejects_metadata_object_overwrite_of_existing_target() {
        let app = test_app();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-existing-metadata-guard-{unique}"));
        let table_dir = root.join("events");
        let metadata_dir = table_dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let table_location = url::Url::from_directory_path(&table_dir)
            .expect("table dir URL")
            .to_string();
        let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
            .unwrap()
            .to_string();
        let target_metadata_path = metadata_dir.join("00001.json");
        let target_metadata_location = url::Url::from_file_path(&target_metadata_path)
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

        let sentinel = "{\n  \"sentinel\": \"existing target must not be overwritten\"\n}\n";
        std::fs::write(&target_metadata_path, sentinel).unwrap();
        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "requirements": [],
                    "updates": [],
                    "metadata-location": target_metadata_location,
                    "metadata": {
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
                                "required": true
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
                    },
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            payload["error"]["message"]
                .as_str()
                .unwrap()
                .contains("refusing to overwrite existing metadata")
        );
        let message = payload["error"]["message"].as_str().unwrap();
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(!message.contains(&target_metadata_location));
        assert!(!message.contains("00001.json"));
        assert_eq!(
            std::fs::read_to_string(&target_metadata_path).unwrap(),
            sentinel
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn commit_rejects_metadata_object_outside_storage_profile_prefix() {
        let app = test_app();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-metadata-prefix-guard-{unique}"));
        let table_dir = root.join("events");
        let metadata_dir = table_dir.join("metadata");
        let outside_dir = root.join("outside");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let table_location = url::Url::from_directory_path(&table_dir)
            .expect("table dir URL")
            .to_string();
        let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
            .unwrap()
            .to_string();
        let outside_metadata_path = outside_dir.join("00001.json");
        let outside_metadata_location = url::Url::from_file_path(&outside_metadata_path)
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

        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "requirements": [],
                    "updates": [],
                    "metadata-location": outside_metadata_location,
                    "metadata": {
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
                                "required": true
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
                    },
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = payload["error"]["message"].as_str().unwrap();
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(message.contains("storage-profile-prefix-hash=sha256:"));
        assert!(!message.contains(&outside_metadata_location));
        assert!(!message.contains(&table_location));
        assert!(!outside_metadata_path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn metadata_write_plan_requires_metadata_location() {
        let table = TableRecord::new(
            table_ident("local", "default", "events").unwrap(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );
        let storage_profile = StorageProfile::inferred_for_table(&table);
        let plan = lakecat_sail::CommitPlan {
            prepared_by: "test".to_string(),
            requirements: Vec::new(),
            updates: Vec::new(),
            new_metadata_location: None,
            new_metadata: serde_json::json!({"format-version": 3}),
            metadata_write_required: true,
            metadata_patch: serde_json::json!({}),
        };

        let err = validate_planned_metadata_location(&plan, None, &storage_profile).unwrap_err();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(
            err.to_string()
                .contains("metadata object commit requires a new metadata location")
        );
    }

    #[test]
    fn metadata_write_plan_rejects_storage_profile_root_location() {
        let table = TableRecord::new(
            table_ident("local", "default", "events").unwrap(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );
        let storage_profile = StorageProfile::inferred_for_table(&table);
        let plan = lakecat_sail::CommitPlan {
            prepared_by: "test".to_string(),
            requirements: Vec::new(),
            updates: Vec::new(),
            new_metadata_location: Some("file:///tmp/events".to_string()),
            new_metadata: serde_json::json!({"format-version": 3}),
            metadata_write_required: true,
            metadata_patch: serde_json::json!({}),
        };

        let err = validate_planned_metadata_location(
            &plan,
            table.metadata_location.as_deref(),
            &storage_profile,
        )
        .unwrap_err();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        let message = err.to_string();
        assert!(message.contains("not a child object"));
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(message.contains("storage-profile-prefix-hash=sha256:"));
        assert!(!message.contains("file:///tmp/events"));
    }

    #[test]
    fn metadata_write_plan_rejects_dot_segment_locations() {
        let table = TableRecord::new(
            table_ident("local", "default", "events").unwrap(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );
        let storage_profile = StorageProfile::inferred_for_table(&table);
        for location in [
            "file:///tmp/events/metadata/../00001.json",
            "file:///tmp/events/metadata/%2e%2e/00001.json",
        ] {
            let plan = lakecat_sail::CommitPlan {
                prepared_by: "test".to_string(),
                requirements: Vec::new(),
                updates: Vec::new(),
                new_metadata_location: Some(location.to_string()),
                new_metadata: serde_json::json!({"format-version": 3}),
                metadata_write_required: true,
                metadata_patch: serde_json::json!({}),
            };

            let err = validate_planned_metadata_location(
                &plan,
                table.metadata_location.as_deref(),
                &storage_profile,
            )
            .unwrap_err();
            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("dot path segments"));
            assert!(message.contains("metadata-location-hash=sha256:"));
            assert!(!message.contains(location));
            assert!(!message.contains("00001.json"));
        }
    }

    #[test]
    fn metadata_write_plan_rejects_query_or_fragment_locations() {
        let table = TableRecord::new(
            table_ident("local", "default", "events").unwrap(),
            "file:///tmp/events".to_string(),
            Some("file:///tmp/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );
        let storage_profile = StorageProfile::inferred_for_table(&table);
        for location in [
            "file:///tmp/events/metadata/00001.json?version=staged",
            "file:///tmp/events/metadata/00001.json#staged",
        ] {
            let plan = lakecat_sail::CommitPlan {
                prepared_by: "test".to_string(),
                requirements: Vec::new(),
                updates: Vec::new(),
                new_metadata_location: Some(location.to_string()),
                new_metadata: serde_json::json!({"format-version": 3}),
                metadata_write_required: true,
                metadata_patch: serde_json::json!({}),
            };

            let err = validate_planned_metadata_location(
                &plan,
                table.metadata_location.as_deref(),
                &storage_profile,
            )
            .unwrap_err();
            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("query strings or fragments"));
            assert!(message.contains("metadata-location-hash=sha256:"));
            assert!(!message.contains(location));
            assert!(!message.contains("00001.json"));
        }
    }

    #[test]
    fn metadata_write_plan_rejects_userinfo_locations() {
        let table = TableRecord::new(
            table_ident("local", "default", "events").unwrap(),
            "s3://lakecat-demo/events".to_string(),
            Some("s3://lakecat-demo/events/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );
        let storage_profile = StorageProfile::new(
            "s3-events",
            WarehouseName::new("local").unwrap(),
            "s3://lakecat-demo/events",
            StorageProvider::S3,
            CredentialIssuanceMode::ShortLivedSecretRef,
            Some("typesec://lakecat/local/s3-events".to_string()),
            BTreeMap::new(),
        )
        .unwrap();
        let location = "s3://access:secret@lakecat-demo/events/metadata/00001.json";
        let plan = lakecat_sail::CommitPlan {
            prepared_by: "test".to_string(),
            requirements: Vec::new(),
            updates: Vec::new(),
            new_metadata_location: Some(location.to_string()),
            new_metadata: serde_json::json!({"format-version": 3}),
            metadata_write_required: true,
            metadata_patch: serde_json::json!({}),
        };

        let err = validate_planned_metadata_location(
            &plan,
            table.metadata_location.as_deref(),
            &storage_profile,
        )
        .unwrap_err();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        let message = err.to_string();
        assert!(message.contains("userinfo"));
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(!message.contains(location));
        assert!(!message.contains("access"));
        assert!(!message.contains("secret"));
        assert!(!message.contains("00001.json"));
    }

    #[test]
    fn location_dot_segment_detection_decodes_percent_encoded_segments() {
        assert!(location_has_dot_path_segment(
            "s3://lakecat/events/metadata/../00001.json"
        ));
        assert!(location_has_dot_path_segment(
            "s3://lakecat/events/metadata/%2E%2e/00001.json"
        ));
        assert!(!location_has_dot_path_segment(
            "s3://lakecat/events/metadata/v1.2/00001.json"
        ));
    }

    #[test]
    fn metadata_cleanup_error_redacts_metadata_location() {
        let location = "file:///tmp/lakecat-secret/events/metadata/00001.json";
        let err = metadata_cleanup_error(
            location,
            "permission denied at /tmp/lakecat-secret/events/metadata/00001.json",
        );
        let message = err.to_string();

        assert!(matches!(err, LakeCatError::Internal(_)));
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(message.contains("error-detail-hash=sha256:"));
        assert!(!message.contains("lakecat-secret"));
        assert!(!message.contains("00001.json"));
        assert!(!message.contains("permission denied"));
    }

    #[test]
    fn metadata_write_error_redacts_backend_detail() {
        let location = "file:///tmp/lakecat-secret/events/metadata/00001.json";
        let err = metadata_object_write_error(
            location,
            "backend write failed for /tmp/lakecat-secret/events/metadata/00001.json",
        );
        let message = err.to_string();

        assert!(matches!(err, LakeCatError::Internal(_)));
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(message.contains("error-detail-hash=sha256:"));
        assert!(!message.contains("lakecat-secret"));
        assert!(!message.contains("00001.json"));
        assert!(!message.contains("backend write failed"));
    }

    #[test]
    fn metadata_object_store_redacts_invalid_location_parse_failures() {
        let location = "not a uri /tmp/lakecat-secret/events/metadata/00001.json";
        let err = metadata_object_store(location).unwrap_err();
        let message = err.to_string();

        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(message.contains("error-detail-hash=sha256:"));
        assert!(!message.contains(location));
        assert!(!message.contains("lakecat-secret"));
        assert!(!message.contains("00001.json"));
        assert!(!message.contains("relative URL"));
    }

    #[test]
    fn metadata_object_store_redacts_unsupported_backend_setup_failures() {
        let location = "ftp://lakecat-secret/events/metadata/00001.json";
        let err = metadata_object_store(location).unwrap_err();
        let message = err.to_string();

        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(message.contains("error-detail-hash=sha256:"));
        assert!(!message.contains(location));
        assert!(!message.contains("lakecat-secret"));
        assert!(!message.contains("00001.json"));
        assert!(!message.contains("ftp"));
    }

    #[test]
    fn metadata_cleanup_failure_preserves_commit_conflict() {
        let err = commit_error_with_cleanup_failure(
            LakeCatError::Conflict("metadata pointer changed".to_string()),
            LakeCatError::Internal("failed to clean up object".to_string()),
        );

        let LakeCatError::Conflict(message) = err else {
            panic!("expected cleanup failure to preserve commit conflict");
        };
        assert!(message.contains("metadata pointer changed"));
        assert!(message.contains("metadata cleanup also failed"));
        assert!(message.contains("failed to clean up object"));
    }

    #[test]
    fn metadata_object_location_must_be_child_of_storage_profile_root() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-metadata-root-guard-{unique}"));
        std::fs::create_dir_all(&root).unwrap();
        let storage_root = url::Url::from_directory_path(&root)
            .expect("storage root URL")
            .to_string();
        let profile = StorageProfile::new(
            "local-files",
            WarehouseName::new("local").unwrap(),
            storage_root.clone(),
            StorageProvider::File,
            CredentialIssuanceMode::LocalFileNoSecret,
            None,
            BTreeMap::new(),
        )
        .unwrap();
        let plan = lakecat_sail::CommitPlan {
            prepared_by: "test".to_string(),
            requirements: Vec::new(),
            updates: Vec::new(),
            new_metadata_location: Some(storage_root.clone()),
            new_metadata: serde_json::json!({"format-version": 3}),
            metadata_write_required: true,
            metadata_patch: serde_json::json!({}),
        };

        let err = validate_planned_metadata_location(&plan, None, &profile).unwrap_err();

        let message = err.to_string();
        assert!(message.contains("metadata-location-hash=sha256:"));
        assert!(message.contains("storage-profile-prefix-hash=sha256:"));
        assert!(!message.contains(&storage_root));
        assert!(!message.contains(root.to_string_lossy().as_ref()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn metadata_cleanup_treats_missing_uncommitted_object_as_clean() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-missing-cleanup-{unique}"));
        std::fs::create_dir_all(&root).unwrap();
        let missing = root.join("metadata").join("00001.json");
        let missing_location = url::Url::from_file_path(&missing).unwrap().to_string();

        cleanup_planned_metadata(
            Some(PlannedMetadataWrite {
                location: missing_location,
            }),
            None,
        )
        .await
        .expect("missing uncommitted metadata object should already be clean");

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn metadata_cleanup_skips_previous_metadata_pointer() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-current-cleanup-{unique}"));
        let metadata_dir = root.join("events").join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let current_metadata = metadata_dir.join("00000.json");
        let sentinel = "{\n  \"sentinel\": \"committed metadata must survive cleanup\"\n}\n";
        std::fs::write(&current_metadata, sentinel).unwrap();
        let current_metadata_location = url::Url::from_file_path(&current_metadata)
            .unwrap()
            .to_string();

        cleanup_planned_metadata(
            Some(PlannedMetadataWrite {
                location: current_metadata_location.clone(),
            }),
            Some(&current_metadata_location),
        )
        .await
        .expect("cleanup should skip the previous committed metadata pointer");

        assert_eq!(
            std::fs::read_to_string(&current_metadata).unwrap(),
            sentinel
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "sail-local")]
    #[tokio::test]
    async fn idempotent_commit_replay_skips_stale_sail_revalidation() {
        let store = MemoryCatalogStore::new();
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            store.clone(),
        ));
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-idempotent-replay-{unique}"));
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
        let advanced_metadata = serde_json::json!({
            "format-version": 3,
            "table-uuid": "11111111-1111-1111-1111-111111111111",
            "location": table_location,
            "last-sequence-number": 8,
            "last-updated-ms": 1710000000100_i64,
            "last-column-id": 2,
            "schemas": [
                {
                    "type": "struct",
                    "schema-id": 1,
                    "fields": [{
                        "id": 1,
                        "name": "id",
                        "type": "string",
                        "required": true
                    }]
                },
                {
                    "type": "struct",
                    "schema-id": 2,
                    "fields": [
                        {
                            "id": 1,
                            "name": "id",
                            "type": "string",
                            "required": true
                        },
                        {
                            "id": 2,
                            "name": "payload",
                            "type": "string",
                            "required": false
                        }
                    ]
                }
            ],
            "current-schema-id": 2,
            "partition-specs": [{"spec-id": 0, "fields": []}],
            "default-spec-id": 0,
            "current-snapshot-id": 43,
            "snapshots": [{
                "snapshot-id": 43,
                "sequence-number": 8,
                "timestamp-ms": 1710000000100_i64,
                "summary": {"operation": "append"},
                "schema-id": 2
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
                    "metadata": base_metadata,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let commit_body = serde_json::json!({
            "requirements": [{
                "type": "assert-current-schema-id",
                "current-schema-id": 1
            }],
            "updates": [],
            "metadata-location": committed_metadata_location,
            "metadata": advanced_metadata,
        })
        .to_string();
        for attempt in 0..2 {
            let commit = Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces/default/tables/events/commit")
                .header("content-type", "application/json")
                .header("x-lakecat-idempotency-key", "commit:events:schema-2")
                .body(Body::from(commit_body.clone()))
                .unwrap();
            let response = app.clone().oneshot(commit).await.unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "idempotent commit attempt {attempt} should replay before Sail validation"
            );
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                payload["metadata-location"],
                serde_json::json!(committed_metadata_location)
            );
            assert_eq!(
                payload["metadata"]["current-schema-id"],
                serde_json::json!(2)
            );
        }

        let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
        let records = store.table_commit_records(&ident, 0, None).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(store.load_table(&ident).await.unwrap().version, 1);
        let _ = std::fs::remove_dir_all(root);
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
        let server = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/servers/prod-server")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "display-name": "Production LakeCat",
                    "endpoint-url": "https://lakecat.example.com"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(server).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let project = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/projects/analytics")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "server-id": "prod-server",
                    "display-name": "Analytics"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(project).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let warehouse = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/projects/analytics/warehouses/local")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "storage-root": "file:///tmp/lakecat-analytics"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(warehouse).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

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
        let graph_nodes = body["graph"]["nodes"].as_array().unwrap();
        let server_node = graph_nodes
            .iter()
            .find(|node| node["id"] == serde_json::json!("lakecat:server:prod-server"))
            .expect("bootstrap graph should include durable server node");
        assert_eq!(server_node["label"], serde_json::json!("Server"));
        assert_eq!(
            server_node["properties"]["displayName"],
            serde_json::json!("Production LakeCat")
        );
        assert_eq!(
            server_node["properties"]["source"],
            serde_json::json!("lakecat-management-records")
        );
        let project_node = graph_nodes
            .iter()
            .find(|node| node["id"] == serde_json::json!("lakecat:project:analytics"))
            .expect("bootstrap graph should include durable project node");
        assert_eq!(project_node["label"], serde_json::json!("Project"));
        assert_eq!(
            project_node["properties"]["serverId"],
            serde_json::json!("prod-server")
        );
        let warehouse_node = graph_nodes
            .iter()
            .find(|node| node["id"] == serde_json::json!("lakecat:warehouse:local"))
            .expect("bootstrap graph should include durable warehouse node");
        assert_eq!(warehouse_node["label"], serde_json::json!("Warehouse"));
        assert_eq!(
            warehouse_node["properties"]["projectId"],
            serde_json::json!("analytics")
        );
        assert_eq!(
            warehouse_node["properties"]["storageRoot"],
            serde_json::json!("file:///tmp/lakecat-analytics")
        );
        let graph_edges = body["graph"]["edges"].as_array().unwrap();
        assert!(graph_edges.iter().any(|edge| edge
            == &serde_json::json!({
                "from": "lakecat:catalog",
                "to": "lakecat:server:prod-server",
                "label": "HAS_SERVER"
            })));
        assert!(graph_edges.iter().any(|edge| edge
            == &serde_json::json!({
                "from": "lakecat:server:prod-server",
                "to": "lakecat:project:analytics",
                "label": "HAS_PROJECT"
            })));
        assert!(graph_edges.iter().any(|edge| edge
            == &serde_json::json!({
                "from": "lakecat:project:analytics",
                "to": "lakecat:warehouse:local",
                "label": "HAS_WAREHOUSE"
            })));
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
        assert_eq!(
            body["manifest"]["querygraph-import"]["view-receipt-evidence"][0]["stable-id"],
            body["views"][0]["stable-id"]
        );
        assert_eq!(
            body["manifest"]["querygraph-import"]["view-receipt-evidence"][0]["view-version"],
            body["views"][0]["view-version"]
        );
        assert!(
            body["manifest"]["querygraph-import"]["view-receipt-evidence"][0]["receipt-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert!(
            body["manifest"]["querygraph-import"]["view-receipt-evidence-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
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
        assert_eq!(body["view-version"], serde_json::json!(1));
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
        assert_eq!(body["views"][0]["view-version"], serde_json::json!(1));
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
        assert_eq!(body["views"][0]["view-version"], serde_json::json!(1));

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
        assert_eq!(body["view-version"], serde_json::json!(1));
        assert_eq!(body["schema-version"], serde_json::json!(1));
        assert_eq!(
            body["properties"]["semantic-domain"],
            serde_json::json!("customer")
        );

        let update = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "sql": "select id from customers where active",
                    "dialect": "sql",
                    "schema-version": 2,
                    "expected-view-version": 1,
                    "properties": {
                        "semantic-domain": "customer"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(update).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["view-version"], serde_json::json!(2));

        let stale_update = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "sql": "select email from customers where active",
                    "dialect": "sql",
                    "schema-version": 3,
                    "expected-view-version": 1
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(stale_update).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let receipts = Request::builder()
            .method(Method::GET)
            .uri(
                "/management/v1/warehouses/local/namespaces/default/views/active_customers/version-receipts",
            )
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(receipts).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let receipts = body["receipts"].as_array().unwrap();
        assert_eq!(receipts.len(), 2);
        assert_eq!(
            receipts[0]["stable-id"],
            serde_json::json!("lakecat:view:local:default:active_customers")
        );
        assert_eq!(receipts[0]["view-version"], serde_json::json!(1));
        assert!(receipts[0]["previous-view-version"].is_null());
        assert!(receipts[0].get("previous-receipt-hash").is_none());
        assert_eq!(receipts[0]["operation"], serde_json::json!("upsert"));
        assert!(
            receipts[0]["receipt-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert_eq!(receipts[1]["view-version"], serde_json::json!(2));
        assert_eq!(receipts[1]["previous-view-version"], serde_json::json!(1));
        assert_eq!(
            receipts[1]["previous-receipt-hash"],
            receipts[0]["receipt-hash"]
        );
        assert_ne!(receipts[0]["view-hash"], receipts[1]["view-hash"]);

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
        assert_eq!(body["view-version"], serde_json::json!(1));
        assert_eq!(body["schema-version"], serde_json::json!(2));

        let catalog_drop = Request::builder()
            .method(Method::DELETE)
            .uri("/catalog/v1/local/namespaces/default/views/catalog_customers?expected-view-version=1")
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

        let stale_management_drop = Request::builder()
            .method(Method::DELETE)
            .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers?expected-view-version=1")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(stale_management_drop).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let receipts_before_drop = Request::builder()
            .method(Method::GET)
            .uri(
                "/management/v1/warehouses/local/namespaces/default/views/active_customers/version-receipts",
            )
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(receipts_before_drop).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["receipts"].as_array().unwrap().len(), 2);

        let management_drop = Request::builder()
            .method(Method::DELETE)
            .uri("/management/v1/warehouses/local/namespaces/default/views/active_customers?expected-view-version=2")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(management_drop).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let receipts_after_drop = Request::builder()
            .method(Method::GET)
            .uri(
                "/management/v1/warehouses/local/namespaces/default/views/active_customers/version-receipts",
            )
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(receipts_after_drop).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let receipts = body["receipts"].as_array().unwrap();
        assert_eq!(receipts.len(), 3);
        assert_eq!(receipts[2]["view-version"], serde_json::json!(2));
        assert_eq!(receipts[2]["previous-view-version"], serde_json::json!(2));
        assert_eq!(
            receipts[2]["previous-receipt-hash"],
            receipts[1]["receipt-hash"]
        );
        assert_eq!(receipts[2]["operation"], serde_json::json!("drop"));
        assert_eq!(receipts[2]["view-hash"], receipts[1]["view-hash"]);
        assert!(
            receipts[2]["receipt-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );

        let chains_after_drop = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses/local/namespaces/default/view-version-receipt-chains")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(chains_after_drop).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let chains = body["chains"].as_array().unwrap();
        assert_eq!(chains.len(), 2);
        let active_chain = chains
            .iter()
            .find(|chain| chain["name"] == serde_json::json!("catalog_customers"))
            .unwrap();
        assert_eq!(active_chain["tombstoned"], serde_json::json!(true));
        assert_eq!(active_chain["latest-operation"], serde_json::json!("drop"));
        assert_eq!(active_chain["chain-verified"], serde_json::json!(true));
        assert!(
            active_chain["chain-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        let dropped_chain = chains
            .iter()
            .find(|chain| chain["name"] == serde_json::json!("active_customers"))
            .unwrap();
        assert_eq!(dropped_chain["tombstoned"], serde_json::json!(true));
        assert_eq!(dropped_chain["latest-view-version"], serde_json::json!(2));
        assert_eq!(dropped_chain["latest-operation"], serde_json::json!("drop"));
        assert_eq!(dropped_chain["receipt-count"], serde_json::json!(3));
        assert_eq!(dropped_chain["chain-verified"], serde_json::json!(true));
        assert!(
            dropped_chain["chain-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert_ne!(active_chain["chain-hash"], dropped_chain["chain-hash"]);

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
    async fn view_mutations_reject_zero_expected_version_without_receipts() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/namespaces/default/views/guarded_view")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "sql": "select id from customers",
                    "dialect": "sql",
                    "schema-version": 1
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let invalid_update = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/namespaces/default/views/guarded_view")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "sql": "select email from customers",
                    "dialect": "sql",
                    "schema-version": 2,
                    "expected-view-version": 0
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(invalid_update).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("expected view version must be greater than zero")
        );

        let invalid_drop = Request::builder()
            .method(Method::DELETE)
            .uri("/management/v1/warehouses/local/namespaces/default/views/guarded_view?expected-view-version=0")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(invalid_drop).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("expected view version must be greater than zero")
        );

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/local/namespaces/default/views/guarded_view")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["view-version"], serde_json::json!(1));
        assert_eq!(body["schema-version"], serde_json::json!(1));

        let receipts = Request::builder()
            .method(Method::GET)
            .uri(
                "/management/v1/warehouses/local/namespaces/default/views/guarded_view/version-receipts",
            )
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(receipts).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let receipts = body["receipts"].as_array().unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0]["operation"], serde_json::json!("upsert"));
        assert_eq!(receipts[0]["view-version"], serde_json::json!(1));
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
    async fn management_storage_profile_rejects_provider_prefix_mismatch() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/wrong-provider")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "s3://lakecat-demo/events",
                    "provider": "file",
                    "issuance-mode": "local-file-no-secret"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("storage-profile-prefix-hash=sha256:"));
        assert!(!message.contains("s3://lakecat-demo/events"));
        assert!(!message.contains("lakecat-demo"));
    }

    #[tokio::test]
    async fn management_storage_profile_rejects_remote_local_no_secret_mode() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/remote-no-secret")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "s3://lakecat-demo/events",
                    "provider": "s3",
                    "issuance-mode": "local-file-no-secret"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("public config value may expose secret material"));
        assert!(message.contains("public-config-key-hash=sha256:"));
        assert!(!message.contains("lakecat.endpoint"));
        assert!(!message.contains("raw-secret"));
    }

    #[tokio::test]
    async fn management_storage_profile_rejects_public_secret_values() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/public-secret")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "s3://lakecat-demo/events",
                    "provider": "s3",
                    "issuance-mode": "short-lived-secret-ref",
                    "secret-ref": "typesec://lakecat/local/s3-events",
                    "public-config": {
                        "lakecat.endpoint": "https://storage.example.invalid?token=raw-secret"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn management_storage_profile_rejects_reserved_public_config_keys() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/reserved-public-config")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "file:///tmp/events",
                    "provider": "file",
                    "issuance-mode": "local-file-no-secret",
                    "public-config": {
                        "lakecat.storage-profile-id": "shadow-profile"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("reserved for LakeCat credential evidence"));
        assert!(message.contains("public-config-key-hash=sha256:"));
        assert!(!message.contains("lakecat.storage-profile-id"));
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
        let body_text = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            !body_text.contains("typesec://lakecat/local/s3-events"),
            "management storage-profile response must not expose raw secret-ref"
        );
        let body: serde_json::Value = serde_json::from_str(&body_text).unwrap();
        assert!(body.get("secret-ref").is_none());
        assert_eq!(body["secret-ref-present"], serde_json::json!(true));
        assert_eq!(body["secret-ref-provider"], serde_json::json!("typesec"));
        assert!(
            body["secret-ref-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        let upsert_secret_ref_hash = body["secret-ref-hash"].clone();

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
        let body_text = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            !body_text.contains("typesec://lakecat/local/s3-events"),
            "management storage-profile list response must not expose raw secret-ref"
        );
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let listed = &body["storage-profiles"][0];
        assert!(listed.get("secret-ref").is_none());
        assert_eq!(listed["secret-ref-present"], serde_json::json!(true));
        assert_eq!(listed["secret-ref-provider"], serde_json::json!("typesec"));
        assert_eq!(listed["secret-ref-hash"], upsert_secret_ref_hash);

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
            max_credential_ttl_seconds: None,
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
            assert!(err.to_string().contains("secret-ref-hash=sha256:"));
            assert!(
                !err.to_string().contains(secret_ref),
                "not-configured resolver errors must not expose the raw secret-ref URI"
            );

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
    async fn typesec_credential_issuer_dispatches_configured_production_secret_backends_after_authorization()
     {
        use crate::typesec_credential_issuer::{
            ExternalSecretRefCredentialResolver, SecretRefCredentialResolver, SecretRefProvider,
            TypeSecCredentialIssuer,
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
            Some("aws-sm://lakecat/s3-events".to_string()),
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
            max_credential_ttl_seconds: Some(300),
        };

        for (provider, provider_label, secret_ref) in [
            (
                SecretRefProvider::AwsSecretsManager,
                "aws-secrets-manager",
                "aws-sm://lakecat/s3-events",
            ),
            (
                SecretRefProvider::GcpSecretManager,
                "gcp-secret-manager",
                "gcp-sm://lakecat/s3-events",
            ),
            (
                SecretRefProvider::AzureKeyVault,
                "azure-key-vault",
                "azure-kv://lakecat/s3-events",
            ),
        ] {
            let backend = Arc::new(MockProductionSecretRefResolver {
                provider_label,
                credential_prefix: None,
                requests: Mutex::new(Vec::new()),
            });
            let mut backends: BTreeMap<SecretRefProvider, Arc<dyn SecretRefCredentialResolver>> =
                BTreeMap::new();
            backends.insert(provider, backend.clone());
            let issuer = TypeSecCredentialIssuer::new(
                Arc::new(AllowCredentialIssuePolicy {
                    subject: "did:example:agent".to_string(),
                    resource: secret_ref.to_string(),
                }),
                ExternalSecretRefCredentialResolver::with_provider_backends(backends),
            );

            let mut allowed_request = request.clone();
            allowed_request.profile.secret_ref = Some(secret_ref.to_string());
            let credentials = issuer.issue(allowed_request.clone()).await.unwrap();
            assert_eq!(credentials.len(), 1);
            assert_eq!(credentials[0].prefix, "s3://lakecat-demo/events");
            assert!(credentials[0].config.iter().any(|entry| {
                entry.key == "lakecat.credential-kind"
                    && entry.value == format!("{provider_label}-short-lived")
            }));
            assert!(credentials[0].config.iter().any(|entry| {
                entry.key == "lakecat.max-credential-ttl-seconds" && entry.value == "300"
            }));
            assert_eq!(
                *backend.requests.lock().await,
                vec![(secret_ref.to_string(), Some(300))]
            );

            let denied = TypeSecCredentialIssuer::new(
                Arc::new(AllowCredentialIssuePolicy {
                    subject: "did:example:other".to_string(),
                    resource: secret_ref.to_string(),
                }),
                ExternalSecretRefCredentialResolver::with_provider_backends(BTreeMap::from([(
                    provider,
                    backend.clone() as Arc<dyn SecretRefCredentialResolver>,
                )])),
            );
            let err = denied.issue(allowed_request).await.unwrap_err();
            assert!(matches!(err, LakeCatError::Conflict(_)));
            assert!(
                err.to_string()
                    .contains("TypeSec denied credential issuance")
            );
            assert_eq!(
                *backend.requests.lock().await,
                vec![(secret_ref.to_string(), Some(300))],
                "denied TypeSec decisions must not dispatch to the production backend"
            );
        }
    }

    #[cfg(feature = "typesec-local")]
    #[tokio::test]
    async fn typesec_credential_issuer_rejects_backend_credentials_outside_profile_scope() {
        use crate::typesec_credential_issuer::{
            ExternalSecretRefCredentialResolver, SecretRefCredentialResolver, SecretRefProvider,
            TypeSecCredentialIssuer,
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
            Some("aws-sm://lakecat/s3-events".to_string()),
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
            max_credential_ttl_seconds: Some(300),
        };
        let backend = Arc::new(MockProductionSecretRefResolver {
            provider_label: "aws-secrets-manager",
            credential_prefix: Some("s3://lakecat-demo"),
            requests: Mutex::new(Vec::new()),
        });
        let issuer = TypeSecCredentialIssuer::new(
            Arc::new(AllowCredentialIssuePolicy {
                subject: "did:example:agent".to_string(),
                resource: "aws-sm://lakecat/s3-events".to_string(),
            }),
            ExternalSecretRefCredentialResolver::with_provider_backends(BTreeMap::from([(
                SecretRefProvider::AwsSecretsManager,
                backend.clone() as Arc<dyn SecretRefCredentialResolver>,
            )])),
        );

        let err = issuer.issue(request).await.unwrap_err();
        assert!(matches!(err, LakeCatError::InvalidArgument(_)));
        let message = err.to_string();
        assert!(message.contains("issued credential prefix is outside storage profile scope"));
        assert!(message.contains("credential-prefix-hash=sha256:"));
        assert!(message.contains("storage-profile-prefix-hash=sha256:"));
        assert!(!message.contains("s3://lakecat-demo"));
        assert_eq!(
            *backend.requests.lock().await,
            vec![("aws-sm://lakecat/s3-events".to_string(), Some(300))],
            "authorized backend dispatch is allowed, but returned credentials must stay scoped"
        );
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
            max_credential_ttl_seconds: None,
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
        for secret_ref in [
            "typesec://env/lowercase",
            "typesec://vault/path",
            "vault://",
            "typesec://env/",
            "not a typesec uri with secret=abc",
        ] {
            let err =
                if secret_ref.starts_with("vault://") || secret_ref.starts_with("not a typesec") {
                    vault_secret_path(secret_ref).unwrap_err()
                } else {
                    env_secret_variable(secret_ref).unwrap_err()
                };
            let message = err.to_string();
            assert!(message.contains("secret-ref-hash=sha256:"));
            assert!(
                !message.contains(secret_ref),
                "resolver validation errors must not expose raw secret refs"
            );
        }
        let malformed_provider_ref = "not a credential ref token=abc";
        let err = secret_ref_provider(malformed_provider_ref).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("secret-ref-hash=sha256:"));
        assert!(
            !message.contains(malformed_provider_ref),
            "provider validation errors must not expose raw secret refs"
        );
        let malformed_env_ref = "not a typesec env ref token=abc";
        let err = env_secret_variable(malformed_env_ref).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("secret-ref-hash=sha256:"));
        assert!(
            !message.contains(malformed_env_ref),
            "environment resolver parse errors must not expose raw secret refs"
        );
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
            event.payload["payload"]["storage-profile"]["profile-id"],
            serde_json::json!("local:file")
        );
        assert_eq!(
            event.payload["payload"]["storage-profile"]["secret-ref-present"],
            serde_json::json!(false)
        );
        assert_eq!(
            event.payload["payload"]["credential-response-evidence"],
            serde_json::json!([])
        );
        assert_eq!(
            event.payload["payload"]["lakecat:credential-block-reason"],
            serde_json::json!("fine-grained read restriction requires Sail-planned reads")
        );
    }

    #[tokio::test]
    async fn credential_vend_rejects_malformed_odrl_before_issuer() {
        let store = MemoryCatalogStore::new();
        let issuer = Arc::new(RecordingCredentialIssuer::default());
        let app = app(
            LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
                .with_credential_issuer(issuer.clone()),
        );
        let table = TableRecord::new(
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
        let ident = table.ident.clone();
        store.create_table(table).await.unwrap();
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
                        "permission": [{
                            "action": "read",
                            "constraint": [{
                                "leftOperand": "allowed-columns",
                                "operator": "eq"
                            }]
                        }]
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("ODRL allowed columns constraint must include a right operand"));
        assert!(
            issuer.requests.lock().await.is_empty(),
            "malformed active ODRL must fail before credential issuer dispatch"
        );
        let outbox = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        assert!(
            outbox
                .iter()
                .all(|event| event.event_type != "credentials.vend-attempted"),
            "malformed active ODRL must not emit credential-vend replay evidence"
        );
    }

    #[tokio::test]
    async fn credential_vend_rejects_issuer_credentials_outside_profile_scope() {
        let store = MemoryCatalogStore::new();
        let issuer = Arc::new(BroadCredentialIssuer::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_credential_issuer(issuer.clone());
        let create = TableRecord::new(
            TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            ),
            "s3://lakecat-demo/events/tenant-a".to_string(),
            Some("s3://lakecat-demo/events/tenant-a/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );
        store.create_table(create).await.unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-lakecat-principal",
            axum::http::HeaderValue::from_static("human:operator"),
        );
        let err = load_credentials(
            State(state),
            headers,
            Path(("default".to_string(), "events".to_string())),
        )
        .await
        .expect_err("broad issuer credentials must be rejected by LakeCat");
        let message = err.0.to_string();
        assert!(matches!(err.0, LakeCatError::InvalidArgument(_)));
        assert!(message.contains("issued credential prefix is outside storage profile scope"));
        assert!(message.contains("credential-prefix-hash=sha256:"));
        assert!(message.contains("storage-profile-prefix-hash=sha256:"));
        assert!(!message.contains("s3://lakecat-demo"));

        let requests = issuer.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].profile.location_prefix,
            "s3://lakecat-demo/events/tenant-a"
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
        assert!(
            response.0.storage_credentials[0]
                .config
                .iter()
                .any(|entry| {
                    entry.key == "lakecat.max-credential-ttl-seconds" && entry.value == "300"
                })
        );

        let requests = issuer.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].authorization_receipt.principal.kind,
            PrincipalKind::Human
        );
        assert_eq!(requests[0].max_credential_ttl_seconds, Some(300));
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
        assert_eq!(
            event.payload["payload"]["storage-profile"]["profile-id"],
            serde_json::json!("local:file")
        );
        assert_eq!(
            event.payload["payload"]["storage-profile"]["secret-ref-present"],
            serde_json::json!(false)
        );
        let response_evidence = event.payload["payload"]["credential-response-evidence"]
            .as_array()
            .expect("credential response evidence should be recorded in outbox");
        assert_eq!(response_evidence.len(), 1);
        assert_eq!(
            response_evidence[0]["storage-profile-id"],
            serde_json::json!("local:file")
        );
        assert_eq!(
            response_evidence[0]["storage-provider"],
            serde_json::json!("file")
        );
        assert_eq!(
            response_evidence[0]["credential-mode"],
            serde_json::json!("local-file-no-secret")
        );
        assert_eq!(
            response_evidence[0]["authorization-principal"],
            serde_json::json!("human:operator")
        );
        assert_eq!(
            response_evidence[0]["governed-read-required"],
            serde_json::json!("true")
        );
        assert_eq!(
            response_evidence[0]["max-credential-ttl-seconds"],
            serde_json::json!("300")
        );
        assert!(
            response_evidence[0]["prefix-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert!(
            response_evidence[0]["issuer-config-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        let evidence_text = serde_json::to_string(&response_evidence).unwrap();
        assert!(!evidence_text.contains("file:///tmp/events"));
        assert!(
            event.payload["payload"]
                .get("lakecat:credential-block-reason")
                .is_none()
        );
    }

    #[tokio::test]
    async fn credential_vend_response_normalizes_duplicate_ttl_entries() {
        let store = MemoryCatalogStore::new();
        let issuer = Arc::new(DuplicateTtlCredentialIssuer::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_credential_issuer(issuer.clone());
        let table = TableRecord::new(
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
                        {"id": 1, "name": "event_id", "type": "string", "required": true}
                    ]
                }]
            }),
            Principal::anonymous(),
        );
        let ident = table.ident.clone();
        store.create_table(table).await.unwrap();
        store
            .upsert_policy_binding(
                PolicyBinding::new(
                    "trusted-human-ttl-cap",
                    WarehouseName::new("local").unwrap(),
                    Some(ident.namespace.clone()),
                    Some(ident.name.clone()),
                    true,
                    serde_json::json!({
                        "uid": "policy:trusted-human-ttl-cap",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"],
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

        let credentials = response.0.storage_credentials;
        assert_eq!(credentials.len(), 1);
        let ttl_entries = credentials[0]
            .config
            .iter()
            .filter(|entry| entry.key == "lakecat.max-credential-ttl-seconds")
            .collect::<Vec<_>>();
        assert_eq!(ttl_entries.len(), 1);
        assert_eq!(ttl_entries[0].value, "120");
        assert!(credentials[0].config.iter().any(|entry| {
            entry.key == "aws.session-token" && entry.value == "temporary-test-token"
        }));

        let requests = issuer.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].max_credential_ttl_seconds, Some(300));
    }

    #[tokio::test]
    async fn credential_vend_response_replaces_shadowed_lakecat_evidence() {
        let store = MemoryCatalogStore::new();
        let issuer = Arc::new(ShadowingCredentialEvidenceIssuer::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_credential_issuer(issuer.clone());
        let table = TableRecord::new(
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
                        {"id": 1, "name": "event_id", "type": "string", "required": true}
                    ]
                }]
            }),
            Principal::anonymous(),
        );
        let ident = table.ident.clone();
        store.create_table(table).await.unwrap();
        store
            .upsert_policy_binding(
                PolicyBinding::new(
                    "trusted-human-shadowed-evidence",
                    WarehouseName::new("local").unwrap(),
                    Some(ident.namespace.clone()),
                    Some(ident.name.clone()),
                    true,
                    serde_json::json!({
                        "uid": "policy:trusted-human-shadowed-evidence",
                        "lakecat:read-restriction": {
                            "allowed-columns": ["event_id"],
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

        let credentials = response.0.storage_credentials;
        assert_eq!(credentials.len(), 1);
        let config = &credentials[0].config;
        assert_single_config_value(config, "lakecat.storage-profile-id", "local:file");
        assert_single_config_value(config, "lakecat.storage-provider", "file");
        assert_single_config_value(config, "lakecat.credential-mode", "local-file-no-secret");
        assert_single_config_value(config, "lakecat.authorization-principal", "human:operator");
        assert_single_config_value(config, "lakecat.governed-read-required", "true");
        assert_single_config_value(config, "lakecat.max-credential-ttl-seconds", "120");
        assert!(config.iter().any(|entry| {
            entry.key == "lakecat.credential-kind" && entry.value == "shadow-test"
        }));
        assert!(config.iter().any(|entry| {
            entry.key == "aws.session-token" && entry.value == "temporary-test-token"
        }));

        let requests = issuer.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].max_credential_ttl_seconds, Some(300));
    }

    fn assert_single_config_value(config: &[ConfigEntry], key: &str, expected: &str) {
        let values = config
            .iter()
            .filter(|entry| entry.key == key)
            .map(|entry| entry.value.as_str())
            .collect::<Vec<_>>();
        assert_eq!(values, vec![expected], "{key} must be canonical");
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

        let credentials = canonicalize_credential_response_evidence(
            vec![StorageCredential {
                prefix: "file:///tmp/events".to_string(),
                config: vec![
                    ConfigEntry::new("lakecat.storage-profile-id", "shadow"),
                    ConfigEntry::new("aws.session-token", "temporary-test-token"),
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "120"),
                ],
            }],
            &profile,
            &receipt,
            true,
            Some(300),
        );

        let payload =
            credentials_vend_audit_payload(&ident, &table, &profile, &credentials, &receipt)
                .unwrap();
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
        assert_eq!(
            payload["storage-profile"]["location-prefix-hash"],
            serde_json::json!(
                content_hash_json(&json!({"location-prefix": "file:///tmp/events"})).unwrap()
            )
        );
        assert!(
            payload["storage-profile"].get("location-prefix").is_none(),
            "credential-vend audit payload must not expose raw storage-profile location prefixes"
        );
        let response_evidence = payload["credential-response-evidence"]
            .as_array()
            .expect("credential response evidence should be an array");
        assert_eq!(response_evidence.len(), 1);
        assert_eq!(
            response_evidence[0]["storage-profile-id"],
            serde_json::json!("local:file")
        );
        assert_eq!(
            response_evidence[0]["storage-provider"],
            serde_json::json!("file")
        );
        assert_eq!(
            response_evidence[0]["credential-mode"],
            serde_json::json!("local-file-no-secret")
        );
        assert_eq!(
            response_evidence[0]["authorization-principal"],
            serde_json::json!("did:example:agent")
        );
        assert_eq!(
            response_evidence[0]["governed-read-required"],
            serde_json::json!("true")
        );
        assert_eq!(
            response_evidence[0]["max-credential-ttl-seconds"],
            serde_json::json!("120")
        );
        assert!(
            response_evidence[0]["prefix-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        assert!(
            response_evidence[0]["issuer-config-hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("sha256:"))
        );
        let evidence_text = serde_json::to_string(&response_evidence).unwrap();
        assert!(!evidence_text.contains("temporary-test-token"));
        assert!(!evidence_text.contains("file:///tmp/events"));
    }

    #[test]
    fn credential_ttl_cap_preserves_stricter_issuer_ttl() {
        let credentials = apply_credential_ttl_cap(
            vec![StorageCredential {
                prefix: "s3://lakecat-demo/events".to_string(),
                config: vec![
                    ConfigEntry::new("lakecat.credential-kind", "issuer-short-lived"),
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "60"),
                ],
            }],
            Some(300),
        );

        let ttl_entries = credentials[0]
            .config
            .iter()
            .filter(|entry| entry.key == "lakecat.max-credential-ttl-seconds")
            .collect::<Vec<_>>();
        assert_eq!(ttl_entries.len(), 1);
        assert_eq!(
            ttl_entries[0].value, "60",
            "issuer TTLs stricter than policy maximum must not be widened"
        );
    }

    #[test]
    fn credential_ttl_cap_collapses_duplicate_issuer_ttl_entries() {
        let credentials = apply_credential_ttl_cap(
            vec![StorageCredential {
                prefix: "s3://lakecat-demo/events".to_string(),
                config: vec![
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "600"),
                    ConfigEntry::new("aws.session-token", "temporary"),
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "120"),
                    ConfigEntry::new("lakecat.max-credential-ttl-seconds", "not-a-number"),
                ],
            }],
            Some(300),
        );

        let ttl_entries = credentials[0]
            .config
            .iter()
            .filter(|entry| entry.key == "lakecat.max-credential-ttl-seconds")
            .collect::<Vec<_>>();
        assert_eq!(ttl_entries.len(), 1);
        assert_eq!(ttl_entries[0].value, "120");
        assert!(
            credentials[0]
                .config
                .iter()
                .any(|entry| { entry.key == "aws.session-token" && entry.value == "temporary" })
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

        let scan_request_extensions = serde_json::json!({
            "requested-stats-fields": ["event_id", "payload"],
            "effective-stats-fields": ["event_id"]
        });
        let payload = table_scan_planned_audit_payload(
            &ident,
            &table,
            &receipt,
            &scan,
            &scan_request_extensions,
        );
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
        assert_eq!(
            payload["requested-stats-fields"],
            serde_json::json!(["event_id", "payload"])
        );
        assert_eq!(
            payload["effective-stats-fields"],
            serde_json::json!(["event_id"])
        );
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
        assert_eq!(
            payload["required-projection"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            payload["required-filters"][0],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
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

    #[cfg(not(feature = "sail-local"))]
    #[tokio::test]
    async fn scan_planning_route_sends_effective_policy_scope_to_sail() {
        let store = MemoryCatalogStore::new();
        let sail = Arc::new(CapturingSailEngine::default());
        let app = app(
            LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
                .with_integrations(
                    sail.clone(),
                    AllowAllGovernanceEngine::new(),
                    NoopCatalogGraphSink::new(),
                    HashOnlyLineageSink::new(),
                ),
        );

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
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
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
                    "stats-fields": ["event_id", "payload"],
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
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["requested-projection"],
            serde_json::json!(["event_id", "payload"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["effective-projection"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["requested-stats-fields"],
            serde_json::json!(["event_id", "payload"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["effective-stats-fields"],
            serde_json::json!(["event_id"])
        );

        let captured = sail
            .last_scan
            .lock()
            .await
            .clone()
            .expect("scan should reach Sail");
        assert_eq!(captured.projection, vec!["event_id".to_string()]);
        assert_eq!(
            captured.filters,
            vec![serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })]
        );

        let ident = table_ident("local", "default", "events").unwrap();
        let outbox = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        let event = outbox
            .iter()
            .find(|event| event.event_type == "table.scan-planned")
            .expect("scan planning should be audited for replay");
        assert_eq!(event.payload["payload"]["table"], serde_json::json!(ident));
        assert_eq!(
            event.payload["payload"]["requested-stats-fields"],
            serde_json::json!(["event_id", "payload"])
        );
        assert_eq!(
            event.payload["payload"]["effective-stats-fields"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            event.payload["payload"]["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
    }

    #[cfg(not(feature = "sail-local"))]
    #[tokio::test]
    async fn scan_planning_rejects_malformed_odrl_before_sail() {
        let store = MemoryCatalogStore::new();
        let sail = Arc::new(CapturingSailEngine::default());
        let app = app(
            LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
                .with_integrations(
                    sail.clone(),
                    AllowAllGovernanceEngine::new(),
                    NoopCatalogGraphSink::new(),
                    HashOnlyLineageSink::new(),
                ),
        );

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
                        "permission": [{
                            "action": "read",
                            "constraint": [{
                                "leftOperand": "allowed-columns",
                                "operator": "eq"
                            }]
                        }]
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
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
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
        let response = app.oneshot(plan).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("ODRL allowed columns constraint must include a right operand"));
        assert!(
            sail.last_scan.lock().await.is_none(),
            "malformed active ODRL must fail before Sail planning"
        );
        let outbox = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        assert!(
            outbox
                .iter()
                .all(|event| event.event_type != "table.scan-planned"),
            "malformed active ODRL must not emit scan-planned replay evidence"
        );
    }

    #[cfg(not(feature = "sail-local"))]
    #[tokio::test]
    async fn scan_planning_rejects_malformed_jsonld_odrl_before_sail() {
        let store = MemoryCatalogStore::new();
        let sail = Arc::new(CapturingSailEngine::default());
        let app = app(
            LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
                .with_integrations(
                    sail.clone(),
                    AllowAllGovernanceEngine::new(),
                    NoopCatalogGraphSink::new(),
                    HashOnlyLineageSink::new(),
                ),
        );

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
                        "permission": [{
                            "action": "read",
                            "constraint": [{
                                "leftOperand": { "@id": "lakecat:allowed-columns" },
                                "operator": { "@id": "odrl:isAnyOf" },
                                "rightOperand": {
                                    "@list": [
                                        { "@value": "event_id" },
                                        { "@id": "lakecat:not-a-column-value" }
                                    ]
                                }
                            }]
                        }]
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
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
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
        let response = app.oneshot(plan).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("ODRL allowed columns must be strings"));
        assert!(
            sail.last_scan.lock().await.is_none(),
            "malformed JSON-LD active ODRL must fail before Sail planning"
        );
        let outbox = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        assert!(
            outbox
                .iter()
                .all(|event| event.event_type != "table.scan-planned"),
            "malformed JSON-LD active ODRL must not emit scan-planned replay evidence"
        );
    }

    #[cfg(not(feature = "sail-local"))]
    #[tokio::test]
    async fn fetch_scan_tasks_route_sends_required_policy_scope_to_sail() {
        let store = MemoryCatalogStore::new();
        let sail = Arc::new(CapturingSailEngine::default());
        let app = app(
            LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
                .with_integrations(
                    sail.clone(),
                    AllowAllGovernanceEngine::new(),
                    NoopCatalogGraphSink::new(),
                    HashOnlyLineageSink::new(),
                ),
        );

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
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let fetch = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/tasks")
            .header("content-type", "application/json")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::from(
                serde_json::json!({"plan-task": "lakecat:plan:captured"}).to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(fetch).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["residual-filter"]["required-projection"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["residual-filter"]["required-filters"][0],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );
        assert_eq!(
            body["residual-filter"]["lakecat:fetch-scan-tasks"]["required-projection"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:fetch-scan-tasks"]["required-filters"][0],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );

        let captured = sail
            .last_fetch
            .lock()
            .await
            .clone()
            .expect("fetch should reach Sail");
        assert_eq!(captured.required_projection, vec!["event_id".to_string()]);
        assert_eq!(
            captured.required_filters,
            vec![serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })]
        );

        let ident = table_ident("local", "default", "events").unwrap();
        let outbox = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        let event = outbox
            .iter()
            .find(|event| event.event_type == "table.scan-tasks-fetched")
            .expect("scan-task fetch should be audited for replay");
        assert_eq!(event.payload["payload"]["table"], serde_json::json!(ident));
        assert_eq!(
            event.payload["payload"]["required-projection"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            event.payload["payload"]["required-filters"][0],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );
        assert_eq!(
            event.payload["payload"]["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
        );
    }

    #[cfg(not(feature = "sail-local"))]
    #[tokio::test]
    async fn fetch_scan_tasks_rejects_malformed_odrl_before_sail() {
        let store = MemoryCatalogStore::new();
        let sail = Arc::new(CapturingSailEngine::default());
        let app = app(
            LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
                .with_integrations(
                    sail.clone(),
                    AllowAllGovernanceEngine::new(),
                    NoopCatalogGraphSink::new(),
                    HashOnlyLineageSink::new(),
                ),
        );

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
                        "permission": [{
                            "action": "read",
                            "constraint": [{
                                "leftOperand": "allowed-columns",
                                "operator": "eq"
                            }]
                        }]
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
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let fetch = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/tasks")
            .header("content-type", "application/json")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::from(
                serde_json::json!({"plan-task": "lakecat:plan:captured"}).to_string(),
            ))
            .unwrap();
        let response = app.oneshot(fetch).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("ODRL allowed columns constraint must include a right operand"));
        assert!(
            sail.last_fetch.lock().await.is_none(),
            "malformed active ODRL must fail before Sail fetch"
        );
        let outbox = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        assert!(
            outbox
                .iter()
                .all(|event| event.event_type != "table.scan-tasks-fetched"),
            "malformed active ODRL must not emit scan-task fetch replay evidence"
        );
    }

    #[cfg(not(feature = "sail-local"))]
    #[tokio::test]
    async fn fetch_scan_tasks_rejects_malformed_jsonld_odrl_before_sail() {
        let store = MemoryCatalogStore::new();
        let sail = Arc::new(CapturingSailEngine::default());
        let app = app(
            LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
                .with_integrations(
                    sail.clone(),
                    AllowAllGovernanceEngine::new(),
                    NoopCatalogGraphSink::new(),
                    HashOnlyLineageSink::new(),
                ),
        );

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
                        "permission": [{
                            "action": "read",
                            "constraint": [{
                                "leftOperand": { "@id": "lakecat:allowed-columns" },
                                "operator": { "@id": "odrl:isAnyOf" },
                                "rightOperand": {
                                    "@list": [
                                        { "@value": "event_id" },
                                        { "@id": "lakecat:not-a-column-value" }
                                    ]
                                }
                            }]
                        }]
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
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true},{"id":2,"name":"payload","type":"string","required":false}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let fetch = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/tasks")
            .header("content-type", "application/json")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::from(
                serde_json::json!({"plan-task": "lakecat:plan:captured"}).to_string(),
            ))
            .unwrap();
        let response = app.oneshot(fetch).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("ODRL allowed columns must be strings"));
        assert!(
            sail.last_fetch.lock().await.is_none(),
            "malformed JSON-LD active ODRL must fail before Sail fetch"
        );
        let outbox = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        assert!(
            outbox
                .iter()
                .all(|event| event.event_type != "table.scan-tasks-fetched"),
            "malformed JSON-LD active ODRL must not emit scan-task fetch replay evidence"
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
                    "stats-fields": ["event_id", "payload"],
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
            body["residual-filter"]["lakecat:scan-request"]["requested-stats-fields"],
            serde_json::json!(["event_id", "payload"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["effective-stats-fields"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["stats-fields"],
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
        assert_eq!(
            body["residual-filter"]["lakecat:fetch-scan-tasks"]["required-projection"],
            serde_json::json!(["event_id"])
        );
        assert_eq!(
            body["residual-filter"]["lakecat:fetch-scan-tasks"]["required-filters"][0],
            serde_json::json!({
                "type": "eq",
                "term": "event_id",
                "value": "evt-1"
            })
        );
        assert_eq!(
            body["residual-filter"]["lakecat:fetch-scan-tasks"]["read-restriction"]["allowed-columns"],
            serde_json::json!(["event_id"])
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
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = payload["error"]["message"].as_str().unwrap();
        assert!(!message.contains(&initial_metadata_location));
        assert!(!message.contains(&rejected_metadata_location));
        assert!(!message.contains("00000.json"));
        assert!(!message.contains("00001.json"));
        assert!(!rejected_metadata_path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "sail-local")]
    #[tokio::test]
    async fn cas_race_cleans_up_uncommitted_metadata_file_with_redacted_conflict() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-cas-cleanup-{unique}"));
        let table_dir = root.join("events");
        let metadata_dir = table_dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let table_location = url::Url::from_directory_path(&table_dir)
            .expect("table dir URL")
            .to_string();
        let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
            .unwrap()
            .to_string();
        let racing_metadata_location =
            url::Url::from_file_path(metadata_dir.join("00001-race.json"))
                .unwrap()
                .to_string();
        let rejected_metadata_path = metadata_dir.join("00002-rejected.json");
        let rejected_metadata_location = url::Url::from_file_path(&rejected_metadata_path)
            .unwrap()
            .to_string();
        let store = CasRaceStore::new(MemoryCatalogStore::new(), racing_metadata_location.clone());
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            store.clone(),
        ));
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
                    "requirements": [],
                    "updates": [],
                    "metadata-location": rejected_metadata_location,
                    "metadata": rejected_metadata,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let message = payload["error"]["message"].as_str().unwrap();
        assert!(message.contains("metadata pointer changed"));
        assert!(message.contains("expected-metadata-location-hash=sha256:"));
        assert!(message.contains("actual-metadata-location-hash=sha256:"));
        assert!(!message.contains(&initial_metadata_location));
        assert!(!message.contains(&racing_metadata_location));
        assert!(!message.contains(&rejected_metadata_location));
        assert!(!message.contains("00000.json"));
        assert!(!message.contains("00001-race.json"));
        assert!(!message.contains("00002-rejected.json"));
        assert!(!rejected_metadata_path.exists());

        let ident = table_ident("local", "default".to_string(), "events".to_string()).unwrap();
        let table = store.load_table(&ident).await.unwrap();
        assert_eq!(
            table.metadata_location.as_deref(),
            Some(racing_metadata_location.as_str())
        );
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
