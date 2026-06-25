use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use lakecat_api::{ConfigEntry, StorageCredential};
use lakecat_core::{LakeCatError, LakeCatResult, content_hash_bytes};
use lakecat_store::CredentialIssuanceMode;
use serde_json::Value;
use typesec::{PolicyEngine, PolicyResult, ResourceId, SubjectId};
use url::Url;

use crate::{CredentialIssuanceRequest, CredentialIssuer, error_detail_hash_context};

#[async_trait]
pub trait SecretRefCredentialResolver: Send + Sync + 'static {
    async fn resolve(
        &self,
        request: &CredentialIssuanceRequest,
    ) -> LakeCatResult<Vec<StorageCredential>>;
}

type SecretRefResolverMap = BTreeMap<SecretRefProvider, Arc<dyn SecretRefCredentialResolver>>;

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
    fn check(&self, _subject: &SubjectId, _action: &str, _resource: &ResourceId) -> PolicyResult {
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
    provider_backends: SecretRefResolverMap,
}

impl ExternalSecretRefCredentialResolver {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            env: EnvironmentSecretRefCredentialResolver::new(),
            vault: VaultSecretRefCredentialResolver::from_env(),
            provider_backends: file_provider_backends_from_env(),
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

    #[cfg(test)]
    pub fn with_file_provider_roots(roots: BTreeMap<SecretRefProvider, PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            env: EnvironmentSecretRefCredentialResolver::new(),
            vault: None,
            provider_backends: file_provider_backends(roots),
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
                backend.resolve(request).await.map_err(|err| {
                    LakeCatError::InvalidArgument(format!(
                        "failed to resolve {} credential secret; {}; {}",
                        provider.as_str(),
                        secret_ref_hash_context(secret_ref),
                        error_detail_hash_context(err),
                    ))
                })
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
            .await
            .map_err(|err| {
                LakeCatError::InvalidArgument(format!(
                    "failed to resolve Vault credential secret; {}; {}",
                    secret_ref_hash_context(secret_ref),
                    error_detail_hash_context(err),
                ))
            })?;
        Ok(vec![StorageCredential {
            prefix: request.profile.location_prefix.clone(),
            config: config_entries_from_vault_secret_json(secret).map_err(|err| {
                LakeCatError::InvalidArgument(format!(
                    "failed to parse Vault credential secret; {}; {}",
                    secret_ref_hash_context(secret_ref),
                    error_detail_hash_context(err),
                ))
            })?,
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
                "failed to resolve environment credential secret; {}; {}",
                secret_ref_hash_context(secret_ref),
                error_detail_hash_context(err),
            ))
        })?;
        Ok(vec![StorageCredential {
            prefix: request.profile.location_prefix.clone(),
            config: config_entries_from_secret_json(&raw).map_err(|err| {
                LakeCatError::InvalidArgument(format!(
                    "failed to parse environment credential secret; {}; {}",
                    secret_ref_hash_context(secret_ref),
                    error_detail_hash_context(err),
                ))
            })?,
        }])
    }
}

pub struct FileSecretRefCredentialResolver {
    provider: SecretRefProvider,
    root: PathBuf,
}

impl FileSecretRefCredentialResolver {
    fn new(provider: SecretRefProvider, root: impl Into<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            provider,
            root: root.into(),
        })
    }

    fn secret_path(&self, secret_ref: &str) -> LakeCatResult<PathBuf> {
        let provider = secret_ref_provider(secret_ref)?;
        if provider != self.provider {
            return Err(LakeCatError::InvalidArgument(format!(
                "file-backed {} resolver received mismatched secret provider; {}",
                self.provider.as_str(),
                secret_ref_hash_context(secret_ref),
            )));
        }
        Ok(self
            .root
            .join(format!("{}.json", secret_ref_hash_hex(secret_ref))))
    }
}

#[async_trait]
impl SecretRefCredentialResolver for FileSecretRefCredentialResolver {
    async fn resolve(
        &self,
        request: &CredentialIssuanceRequest,
    ) -> LakeCatResult<Vec<StorageCredential>> {
        let Some(secret_ref) = request.profile.secret_ref.as_deref() else {
            return Ok(Vec::new());
        };
        let path = self.secret_path(secret_ref)?;
        let raw = std::fs::read_to_string(&path).map_err(|err| {
            LakeCatError::InvalidArgument(format!(
                "failed to read file-backed {} credential secret; {}; {}",
                self.provider.as_str(),
                secret_ref_hash_context(secret_ref),
                error_detail_hash_context(err),
            ))
        })?;
        Ok(vec![StorageCredential {
            prefix: request.profile.location_prefix.clone(),
            config: config_entries_from_secret_json(&raw).map_err(|err| {
                LakeCatError::InvalidArgument(format!(
                    "failed to parse file-backed {} credential secret; {}; {}",
                    self.provider.as_str(),
                    secret_ref_hash_context(secret_ref),
                    error_detail_hash_context(err),
                ))
            })?,
        }])
    }
}

fn file_provider_backends_from_env() -> SecretRefResolverMap {
    let roots = [
        (
            SecretRefProvider::AwsSecretsManager,
            "LAKECAT_AWS_SECRETS_MANAGER_FILE_DIR",
        ),
        (
            SecretRefProvider::GcpSecretManager,
            "LAKECAT_GCP_SECRET_MANAGER_FILE_DIR",
        ),
        (
            SecretRefProvider::AzureKeyVault,
            "LAKECAT_AZURE_KEY_VAULT_FILE_DIR",
        ),
    ]
    .into_iter()
    .filter_map(|(provider, var)| std::env::var_os(var).map(|root| (provider, root.into())))
    .collect::<BTreeMap<_, _>>();
    file_provider_backends(roots)
}

fn file_provider_backends(roots: BTreeMap<SecretRefProvider, PathBuf>) -> SecretRefResolverMap {
    roots
        .into_iter()
        .filter(|(provider, _)| provider.supports_file_backend())
        .map(|(provider, root)| {
            (
                provider,
                FileSecretRefCredentialResolver::new(provider, root)
                    as Arc<dyn SecretRefCredentialResolver>,
            )
        })
        .collect()
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

    fn supports_file_backend(self) -> bool {
        matches!(
            self,
            Self::AwsSecretsManager | Self::GcpSecretManager | Self::AzureKeyVault
        )
    }
}

pub(crate) fn secret_ref_provider(secret_ref: &str) -> LakeCatResult<SecretRefProvider> {
    let url = Url::parse(secret_ref).map_err(|_err| {
        LakeCatError::InvalidArgument(format!(
            "invalid credential secret ref URI; {}",
            secret_ref_hash_context(secret_ref)
        ))
    })?;
    reject_decorated_secret_ref_uri(&url, secret_ref)?;
    match url.scheme() {
        "typesec" if url.host_str() == Some("env") => Ok(SecretRefProvider::TypeSecEnv),
        "typesec" => Ok(SecretRefProvider::TypeSec),
        "vault" => Ok(SecretRefProvider::Vault),
        "aws-sm" => Ok(SecretRefProvider::AwsSecretsManager),
        "gcp-sm" => Ok(SecretRefProvider::GcpSecretManager),
        "azure-kv" => Ok(SecretRefProvider::AzureKeyVault),
        _scheme => Err(LakeCatError::InvalidArgument(format!(
            "unsupported credential secret-ref scheme for TypeSec-gated issuance; {}",
            secret_ref_hash_context(secret_ref)
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
    reject_decorated_secret_ref_uri(&url, secret_ref)?;
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
    let entries = object
        .iter()
        .map(|(key, value)| {
            let Some(value) = value.as_str() else {
                return Err(LakeCatError::InvalidArgument(format!(
                    "Vault credential config value for {key} must be a string"
                )));
            };
            Ok(ConfigEntry::new(key.clone(), value.to_string()))
        })
        .collect::<LakeCatResult<Vec<_>>>()?;
    validate_secret_config_entries(entries)
}

pub(crate) fn env_secret_variable(secret_ref: &str) -> LakeCatResult<String> {
    let url = Url::parse(secret_ref).map_err(|_err| {
        LakeCatError::InvalidArgument(format!(
            "invalid TypeSec secret ref URI; {}",
            secret_ref_hash_context(secret_ref)
        ))
    })?;
    reject_decorated_secret_ref_uri(&url, secret_ref)?;
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

fn reject_decorated_secret_ref_uri(url: &Url, secret_ref: &str) -> LakeCatResult<()> {
    if url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "credential secret ref must not include query strings, fragments, or userinfo; {}",
            secret_ref_hash_context(secret_ref)
        )));
    }
    Ok(())
}

fn secret_ref_hash_context(secret_ref: &str) -> String {
    format!(
        "secret-ref-hash={}",
        content_hash_bytes(secret_ref.as_bytes())
    )
}

#[cfg(test)]
pub(crate) fn secret_ref_hash_file_name(secret_ref: &str) -> String {
    format!("{}.json", secret_ref_hash_hex(secret_ref))
}

fn secret_ref_hash_hex(secret_ref: &str) -> String {
    content_hash_bytes(secret_ref.as_bytes())
        .strip_prefix("sha256:")
        .expect("content_hash_bytes returns sha256-prefixed digests")
        .to_string()
}

pub(crate) fn config_entries_from_secret_json(raw: &str) -> LakeCatResult<Vec<ConfigEntry>> {
    let value: Value = serde_json::from_str(raw).map_err(|err| {
        LakeCatError::InvalidArgument(format!("environment credential secret must be JSON: {err}"))
    })?;
    let entries = match value {
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
            .collect::<LakeCatResult<Vec<_>>>()?,
        Value::Array(entries) => entries
            .into_iter()
            .map(|entry| {
                serde_json::from_value(entry).map_err(|err| {
                    LakeCatError::InvalidArgument(format!(
                        "credential config entries must match ConfigEntry JSON shape: {err}"
                    ))
                })
            })
            .collect::<LakeCatResult<Vec<_>>>()?,
        _ => Err(LakeCatError::InvalidArgument(
            "environment credential secret must be a JSON object or ConfigEntry array".to_string(),
        ))?,
    };
    validate_secret_config_entries(entries)
}

fn validate_secret_config_entries(entries: Vec<ConfigEntry>) -> LakeCatResult<Vec<ConfigEntry>> {
    if entries.iter().any(|entry| entry.key.trim().is_empty()) {
        return Err(LakeCatError::InvalidArgument(
            "credential config keys must not be blank".to_string(),
        ));
    }
    Ok(entries)
}
