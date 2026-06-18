use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lakecat_core::{
    AuditStamp, LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, TableName,
    WarehouseName, content_hash_bytes, content_hash_json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

#[async_trait]
pub trait CatalogStore: Send + Sync + 'static {
    async fn create_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: Namespace,
    ) -> LakeCatResult<()>;
    async fn list_namespaces(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<Namespace>>;
    async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>>;
    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord>;
    async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord>;
    async fn commit_table(
        &self,
        ident: &TableIdent,
        commit: TableCommit,
    ) -> LakeCatResult<TableRecord>;
    async fn table_commit_records(
        &self,
        ident: &TableIdent,
        start_version: u64,
        end_version: Option<u64>,
    ) -> LakeCatResult<Vec<TableCommitRecord>>;
    async fn upsert_server(&self, server: ServerRecord) -> LakeCatResult<ServerRecord> {
        server.validate()?;
        Ok(server)
    }
    async fn list_servers(&self) -> LakeCatResult<Vec<ServerRecord>> {
        Ok(Vec::new())
    }
    async fn upsert_project(&self, project: ProjectRecord) -> LakeCatResult<ProjectRecord> {
        project.validate()?;
        Ok(project)
    }
    async fn list_projects(&self) -> LakeCatResult<Vec<ProjectRecord>> {
        Ok(Vec::new())
    }
    async fn upsert_warehouse(&self, warehouse: WarehouseRecord) -> LakeCatResult<WarehouseRecord> {
        warehouse.validate()?;
        Ok(warehouse)
    }
    async fn load_warehouse(&self, warehouse: &WarehouseName) -> LakeCatResult<WarehouseRecord> {
        self.list_warehouses()
            .await?
            .into_iter()
            .find(|record| record.warehouse == *warehouse)
            .ok_or_else(|| LakeCatError::NotFound {
                object: "warehouse",
                name: warehouse.as_str().to_string(),
            })
    }
    async fn list_warehouses(&self) -> LakeCatResult<Vec<WarehouseRecord>> {
        Ok(Vec::new())
    }
    async fn list_project_warehouses(
        &self,
        project_id: &str,
    ) -> LakeCatResult<Vec<WarehouseRecord>> {
        validate_project_id(project_id)?;
        if !self
            .list_projects()
            .await?
            .iter()
            .any(|project| project.project_id == project_id)
        {
            return Err(LakeCatError::NotFound {
                object: "project",
                name: project_id.to_string(),
            });
        }
        Ok(self
            .list_warehouses()
            .await?
            .into_iter()
            .filter(|warehouse| warehouse.project_id == project_id)
            .collect())
    }
    async fn soft_delete_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord>;
    async fn restore_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord>;
    async fn upsert_storage_profile(
        &self,
        profile: StorageProfile,
    ) -> LakeCatResult<StorageProfile> {
        profile.validate()?;
        Ok(profile)
    }
    async fn list_storage_profiles(
        &self,
        _warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<StorageProfile>> {
        Ok(Vec::new())
    }
    async fn upsert_view(&self, view: ViewRecord) -> LakeCatResult<ViewRecord> {
        view.validate()?;
        Ok(view)
    }
    async fn load_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<ViewRecord> {
        self.list_views(warehouse, namespace)
            .await?
            .into_iter()
            .find(|record| record.name == *view)
            .ok_or_else(|| LakeCatError::NotFound {
                object: "view",
                name: view.as_str().to_string(),
            })
    }
    async fn drop_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<ViewRecord> {
        let record = self.load_view(warehouse, namespace, view).await?;
        Ok(record)
    }
    async fn list_views(
        &self,
        _warehouse: &WarehouseName,
        _namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewRecord>> {
        Ok(Vec::new())
    }
    async fn upsert_policy_binding(&self, binding: PolicyBinding) -> LakeCatResult<PolicyBinding> {
        Ok(binding)
    }
    async fn list_policy_bindings(
        &self,
        _warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        Ok(Vec::new())
    }
    async fn policy_bindings_for_table(
        &self,
        _table: &TableIdent,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        Ok(Vec::new())
    }
    async fn storage_profile_for_table(
        &self,
        table: &TableRecord,
    ) -> LakeCatResult<StorageProfile> {
        Ok(StorageProfile::inferred_for_table(table))
    }
    async fn record_audit_event(&self, _event: CatalogAuditEvent) -> LakeCatResult<()> {
        Ok(())
    }
    async fn pending_outbox_events(
        &self,
        _sink: Option<&str>,
        _limit: usize,
    ) -> LakeCatResult<Vec<OutboxEvent>> {
        Ok(Vec::new())
    }
    async fn mark_outbox_delivered(&self, _event_ids: &[String]) -> LakeCatResult<usize> {
        Ok(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableRecord {
    pub ident: TableIdent,
    pub location: String,
    pub metadata_location: Option<String>,
    pub metadata: Value,
    pub created: AuditStamp,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
}

impl TableRecord {
    pub fn new(
        ident: TableIdent,
        location: String,
        metadata_location: Option<String>,
        metadata: Value,
        principal: Principal,
    ) -> Self {
        let created = AuditStamp::now(principal);
        Self {
            ident,
            location,
            metadata_location,
            metadata,
            updated_at: created.at,
            created,
            version: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableCommit {
    pub requirements: Vec<Value>,
    pub updates: Vec<Value>,
    pub expected_previous_metadata_location: Option<String>,
    pub new_metadata_location: Option<String>,
    pub new_metadata: Option<Value>,
    pub idempotency_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_request_hash: Option<String>,
    pub principal: Principal,
    pub authorization_receipt: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableCommitRecord {
    pub table: TableIdent,
    pub previous_metadata_location: Option<String>,
    pub new_metadata_location: Option<String>,
    pub sequence_number: u64,
    pub principal: Principal,
    pub request_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key_sha256: Option<String>,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerRecord {
    pub server_id: String,
    pub display_name: Option<String>,
    pub endpoint_url: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub created: AuditStamp,
    pub updated_at: DateTime<Utc>,
}

impl ServerRecord {
    pub fn new(
        server_id: impl Into<String>,
        display_name: Option<String>,
        endpoint_url: Option<String>,
        properties: BTreeMap<String, String>,
        principal: Principal,
    ) -> LakeCatResult<Self> {
        let created = AuditStamp::now(principal);
        let record = Self {
            server_id: server_id.into(),
            display_name,
            endpoint_url,
            properties,
            updated_at: created.at,
            created,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> LakeCatResult<()> {
        validate_project_id(&self.server_id)?;
        if let Some(display_name) = self.display_name.as_deref()
            && display_name.trim().is_empty()
        {
            return Err(LakeCatError::InvalidArgument(
                "server display name must not be empty".to_string(),
            ));
        }
        if let Some(endpoint_url) = self.endpoint_url.as_deref()
            && endpoint_url.trim().is_empty()
        {
            return Err(LakeCatError::InvalidArgument(
                "server endpoint URL must not be empty".to_string(),
            ));
        }
        validate_public_config(&self.properties)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub project_id: String,
    pub server_id: Option<String>,
    pub display_name: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub created: AuditStamp,
    pub updated_at: DateTime<Utc>,
}

impl ProjectRecord {
    pub fn new(
        project_id: impl Into<String>,
        server_id: Option<String>,
        display_name: Option<String>,
        properties: BTreeMap<String, String>,
        principal: Principal,
    ) -> LakeCatResult<Self> {
        let created = AuditStamp::now(principal);
        let record = Self {
            project_id: project_id.into(),
            server_id,
            display_name,
            properties,
            updated_at: created.at,
            created,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> LakeCatResult<()> {
        validate_project_id(&self.project_id)?;
        if let Some(server_id) = self.server_id.as_deref() {
            validate_project_id(server_id)?;
        }
        if let Some(display_name) = self.display_name.as_deref()
            && display_name.trim().is_empty()
        {
            return Err(LakeCatError::InvalidArgument(
                "project display name must not be empty".to_string(),
            ));
        }
        validate_public_config(&self.properties)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseRecord {
    pub warehouse: WarehouseName,
    pub project_id: String,
    pub storage_root: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub created: AuditStamp,
    pub updated_at: DateTime<Utc>,
}

impl WarehouseRecord {
    pub fn new(
        warehouse: WarehouseName,
        project_id: impl Into<String>,
        storage_root: Option<String>,
        properties: BTreeMap<String, String>,
        principal: Principal,
    ) -> LakeCatResult<Self> {
        let created = AuditStamp::now(principal);
        let record = Self {
            warehouse,
            project_id: project_id.into(),
            storage_root,
            properties,
            updated_at: created.at,
            created,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> LakeCatResult<()> {
        validate_project_id(&self.project_id)?;
        if let Some(storage_root) = self.storage_root.as_deref()
            && storage_root.trim().is_empty()
        {
            return Err(LakeCatError::InvalidArgument(
                "warehouse storage root must not be empty".to_string(),
            ));
        }
        validate_public_config(&self.properties)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViewRecord {
    pub warehouse: WarehouseName,
    pub namespace: Namespace,
    pub name: TableName,
    pub sql: String,
    pub dialect: String,
    pub schema_version: Option<u64>,
    pub properties: BTreeMap<String, String>,
    pub created: AuditStamp,
    pub updated_at: DateTime<Utc>,
}

impl ViewRecord {
    pub fn new(
        warehouse: WarehouseName,
        namespace: Namespace,
        name: TableName,
        sql: impl Into<String>,
        dialect: impl Into<String>,
        schema_version: Option<u64>,
        properties: BTreeMap<String, String>,
        principal: Principal,
    ) -> LakeCatResult<Self> {
        let created = AuditStamp::now(principal);
        let record = Self {
            warehouse,
            namespace,
            name,
            sql: sql.into(),
            dialect: dialect.into(),
            schema_version,
            properties,
            updated_at: created.at,
            created,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> LakeCatResult<()> {
        if self.sql.trim().is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "view SQL must not be empty".to_string(),
            ));
        }
        let dialect = self.dialect.trim();
        if dialect.is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "view dialect must not be empty".to_string(),
            ));
        }
        if dialect.contains(char::is_whitespace) {
            return Err(LakeCatError::InvalidArgument(
                "view dialect must not contain whitespace".to_string(),
            ));
        }
        validate_public_config(&self.properties)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoftDeleteRecord {
    pub table: TableIdent,
    pub metadata_location: Option<String>,
    pub version: u64,
    pub principal: Principal,
    pub authorization_receipt: Option<Value>,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct StorageProfile {
    pub profile_id: String,
    pub warehouse: WarehouseName,
    pub location_prefix: String,
    pub provider: StorageProvider,
    pub issuance_mode: CredentialIssuanceMode,
    pub secret_ref: Option<String>,
    pub public_config: BTreeMap<String, String>,
}

impl StorageProfile {
    pub fn new(
        profile_id: impl Into<String>,
        warehouse: WarehouseName,
        location_prefix: impl Into<String>,
        provider: StorageProvider,
        issuance_mode: CredentialIssuanceMode,
        secret_ref: Option<String>,
        public_config: BTreeMap<String, String>,
    ) -> LakeCatResult<Self> {
        let profile_id = profile_id.into();
        validate_profile_id(&profile_id)?;
        let location_prefix = location_prefix.into();
        if location_prefix.trim().is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "storage profile location prefix must not be empty".to_string(),
            ));
        }
        if let Some(secret_ref) = secret_ref.as_deref() {
            validate_secret_ref(secret_ref)?;
        }
        if matches!(issuance_mode, CredentialIssuanceMode::ShortLivedSecretRef)
            && secret_ref.is_none()
        {
            return Err(LakeCatError::InvalidArgument(
                "short-lived-secret-ref issuance mode requires a secret reference".to_string(),
            ));
        }
        validate_public_config(&public_config)?;
        Ok(Self {
            profile_id,
            warehouse,
            location_prefix,
            provider,
            issuance_mode,
            secret_ref,
            public_config,
        })
    }

    pub fn validate(&self) -> LakeCatResult<()> {
        if let Some(secret_ref) = self.secret_ref.as_deref() {
            validate_secret_ref(secret_ref)?;
        }
        if matches!(
            self.issuance_mode,
            CredentialIssuanceMode::ShortLivedSecretRef
        ) && self.secret_ref.is_none()
        {
            return Err(LakeCatError::InvalidArgument(
                "short-lived-secret-ref issuance mode requires a secret reference".to_string(),
            ));
        }
        Ok(())
    }

    pub fn inferred_for_table(table: &TableRecord) -> Self {
        let provider = StorageProvider::from_location(&table.location);
        let issuance_mode = match provider {
            StorageProvider::File => CredentialIssuanceMode::LocalFileNoSecret,
            StorageProvider::S3
            | StorageProvider::Gcs
            | StorageProvider::Azure
            | StorageProvider::Unknown => CredentialIssuanceMode::GovernedReadRequired,
        };
        let mut public_config = BTreeMap::new();
        public_config.insert(
            "lakecat.credential-mode".to_string(),
            issuance_mode.as_str().to_string(),
        );
        public_config.insert(
            "lakecat.storage-provider".to_string(),
            provider.as_str().to_string(),
        );
        public_config.insert(
            "lakecat.governed-read-required".to_string(),
            matches!(issuance_mode, CredentialIssuanceMode::GovernedReadRequired).to_string(),
        );
        Self {
            profile_id: format!("{}:{}", table.ident.warehouse.as_str(), provider.as_str()),
            warehouse: table.ident.warehouse.clone(),
            location_prefix: table.location.clone(),
            provider,
            issuance_mode,
            secret_ref: None,
            public_config,
        }
    }

    pub fn can_return_public_credential(&self) -> bool {
        matches!(
            self.issuance_mode,
            CredentialIssuanceMode::LocalFileNoSecret
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StorageProvider {
    File,
    S3,
    Gcs,
    Azure,
    Unknown,
}

impl StorageProvider {
    pub fn from_location(location: &str) -> Self {
        if location.starts_with("file://") {
            Self::File
        } else if location.starts_with("s3://") || location.starts_with("s3a://") {
            Self::S3
        } else if location.starts_with("gs://") {
            Self::Gcs
        } else if location.starts_with("az://")
            || location.starts_with("abfs://")
            || location.starts_with("abfss://")
        {
            Self::Azure
        } else {
            Self::Unknown
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::S3 => "s3",
            Self::Gcs => "gcs",
            Self::Azure => "azure",
            Self::Unknown => "unknown",
        }
    }
}

impl FromStr for StorageProvider {
    type Err = LakeCatError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "file" | "local-file" => Ok(Self::File),
            "s3" | "s3a" => Ok(Self::S3),
            "gcs" | "gs" => Ok(Self::Gcs),
            "azure" | "az" | "abfs" | "abfss" => Ok(Self::Azure),
            "unknown" => Ok(Self::Unknown),
            other => Err(LakeCatError::InvalidArgument(format!(
                "unknown storage provider: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialIssuanceMode {
    GovernedReadRequired,
    LocalFileNoSecret,
    ShortLivedSecretRef,
}

impl CredentialIssuanceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GovernedReadRequired => "governed-read-required",
            Self::LocalFileNoSecret => "local-file-no-secret",
            Self::ShortLivedSecretRef => "short-lived-secret-ref",
        }
    }
}

impl FromStr for CredentialIssuanceMode {
    type Err = LakeCatError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "governed-read-required" | "governed" => Ok(Self::GovernedReadRequired),
            "local-file-no-secret" | "no-secret" => Ok(Self::LocalFileNoSecret),
            "short-lived-secret-ref" | "secret-ref" | "short-lived" => {
                Ok(Self::ShortLivedSecretRef)
            }
            other => Err(LakeCatError::InvalidArgument(format!(
                "unknown credential issuance mode: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutboxEvent {
    pub event_id: String,
    pub sink: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogAuditEvent {
    pub event_type: String,
    pub table: Option<TableIdent>,
    pub principal: Principal,
    pub request_hash: Option<String>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyBinding {
    pub policy_id: String,
    pub warehouse: WarehouseName,
    pub namespace: Option<Namespace>,
    pub table: Option<TableName>,
    pub enforced: bool,
    pub odrl: Value,
    pub updated_at: DateTime<Utc>,
}

impl PolicyBinding {
    pub fn new(
        policy_id: impl Into<String>,
        warehouse: WarehouseName,
        namespace: Option<Namespace>,
        table: Option<TableName>,
        enforced: bool,
        odrl: Value,
    ) -> LakeCatResult<Self> {
        let policy_id = policy_id.into();
        validate_policy_id(&policy_id)?;
        if table.is_some() && namespace.is_none() {
            return Err(LakeCatError::InvalidArgument(
                "table-scoped policy binding requires namespace".to_string(),
            ));
        }
        Ok(Self {
            policy_id,
            warehouse,
            namespace,
            table,
            enforced,
            odrl,
            updated_at: Utc::now(),
        })
    }

    pub fn applies_to_table(&self, table: &TableIdent) -> bool {
        if self.warehouse != table.warehouse {
            return false;
        }
        match (&self.namespace, &self.table) {
            (None, None) => true,
            (Some(namespace), None) => namespace == &table.namespace,
            (Some(namespace), Some(table_name)) => {
                namespace == &table.namespace && table_name == &table.name
            }
            (None, Some(_)) => false,
        }
    }
}

impl CatalogAuditEvent {
    pub fn new(
        event_type: impl Into<String>,
        table: Option<TableIdent>,
        principal: Principal,
        payload: Value,
    ) -> LakeCatResult<Self> {
        let request_hash = Some(content_hash_json(&payload)?);
        Ok(Self {
            event_type: event_type.into(),
            table,
            principal,
            request_hash,
            payload,
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Default)]
pub struct MemoryCatalogStore {
    state: RwLock<MemoryState>,
}

impl MemoryCatalogStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[derive(Debug, Default)]
struct MemoryState {
    servers: BTreeMap<String, ServerRecord>,
    projects: BTreeMap<String, ProjectRecord>,
    warehouses: BTreeMap<String, WarehouseRecord>,
    namespaces: BTreeMap<String, BTreeSet<Namespace>>,
    tables: BTreeMap<String, TableRecord>,
    commits: Vec<TableCommitRecord>,
    audit_events: Vec<CatalogAuditEvent>,
    outbox_events: Vec<OutboxEvent>,
    idempotency: BTreeMap<String, IdempotencyReplay>,
    storage_profiles: BTreeMap<String, StorageProfile>,
    views: BTreeMap<String, ViewRecord>,
    policy_bindings: BTreeMap<String, PolicyBinding>,
    soft_deletes: BTreeMap<String, SoftDeleteRecord>,
}

#[derive(Debug, Clone)]
struct IdempotencyReplay {
    request_hash: String,
    response: TableRecord,
}

#[async_trait]
impl CatalogStore for MemoryCatalogStore {
    async fn create_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: Namespace,
    ) -> LakeCatResult<()> {
        let mut state = self.state.write().await;
        state
            .namespaces
            .entry(warehouse.as_str().to_string())
            .or_default()
            .insert(namespace);
        Ok(())
    }

    async fn list_namespaces(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<Namespace>> {
        let state = self.state.read().await;
        Ok(state
            .namespaces
            .get(warehouse.as_str())
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default())
    }

    async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>> {
        let state = self.state.read().await;
        Ok(state
            .tables
            .values()
            .filter(|table| table.ident.warehouse == *warehouse)
            .filter(|table| !state.soft_deletes.contains_key(&table_key(&table.ident)))
            .cloned()
            .collect())
    }

    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord> {
        let mut state = self.state.write().await;
        let warehouse = table.ident.warehouse.as_str().to_string();
        let namespace = table.ident.namespace.clone();
        state
            .namespaces
            .entry(warehouse)
            .or_default()
            .insert(namespace);

        let key = table_key(&table.ident);
        if state.tables.contains_key(&key) {
            return Err(LakeCatError::Conflict(format!(
                "table already exists: {}",
                table.ident.stable_id()
            )));
        }
        state.tables.insert(key, table.clone());
        Ok(table)
    }

    async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord> {
        let state = self.state.read().await;
        state
            .tables
            .get(&table_key(ident))
            .filter(|_| !state.soft_deletes.contains_key(&table_key(ident)))
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })
    }

    async fn commit_table(
        &self,
        ident: &TableIdent,
        commit: TableCommit,
    ) -> LakeCatResult<TableRecord> {
        let mut state = self.state.write().await;
        let key = table_key(ident);
        if state.soft_deletes.contains_key(&key) {
            return Err(LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            });
        }

        let request_hash = content_hash_json(&serde_json::json!({
            "requirements": &commit.requirements,
            "updates": &commit.updates,
            "expected_previous_metadata_location": &commit.expected_previous_metadata_location,
            "new_metadata_location": &commit.new_metadata_location,
            "new_metadata": &commit.new_metadata,
        }))?;
        let idempotency_request_hash = commit
            .idempotency_request_hash
            .clone()
            .unwrap_or_else(|| request_hash.clone());
        if let Some(idempotency_key) = &commit.idempotency_key {
            let idem_key = format!("{}:{idempotency_key}", ident.stable_id());
            if let Some(replay) = state.idempotency.get(&idem_key) {
                if replay.request_hash == idempotency_request_hash {
                    return Ok(replay.response.clone());
                }
                return Err(LakeCatError::Conflict(format!(
                    "idempotency key reused with different commit request for {}",
                    ident.stable_id()
                )));
            }
        }
        let idempotency_key_sha256 = commit
            .idempotency_key
            .as_ref()
            .map(|key| content_hash_bytes(key.as_bytes()));
        let (
            previous_metadata_location,
            new_metadata_location,
            sequence_number,
            committed_at,
            table,
        ) = {
            let table = state
                .tables
                .get_mut(&key)
                .ok_or_else(|| LakeCatError::NotFound {
                    object: "table",
                    name: ident.stable_id(),
                })?;
            let previous_metadata_location = table.metadata_location.clone();
            if previous_metadata_location != commit.expected_previous_metadata_location {
                return Err(LakeCatError::Conflict(format!(
                    "metadata pointer changed for {}",
                    ident.stable_id()
                )));
            }
            table.metadata_location = commit.new_metadata_location.clone();
            if let Some(new_metadata) = commit.new_metadata {
                table.metadata = new_metadata;
            }
            table.version += 1;
            table.updated_at = Utc::now();
            table.metadata["lakecat:version"] = serde_json::json!(table.version);
            table.metadata["lakecat:last-request-hash"] = serde_json::json!(request_hash);
            (
                previous_metadata_location,
                commit.new_metadata_location.clone(),
                table.version,
                table.updated_at,
                table.clone(),
            )
        };

        let record = TableCommitRecord {
            table: ident.clone(),
            previous_metadata_location,
            new_metadata_location,
            sequence_number,
            principal: commit.principal.clone(),
            request_hash,
            idempotency_key_sha256,
            committed_at,
        };
        let replay_request_hash = record.request_hash.clone();
        state.commits.push(record);

        if let Some(idempotency_key) = commit.idempotency_key {
            state.idempotency.insert(
                format!("{}:{idempotency_key}", ident.stable_id()),
                IdempotencyReplay {
                    request_hash: commit
                        .idempotency_request_hash
                        .unwrap_or(replay_request_hash),
                    response: table.clone(),
                },
            );
        }
        Ok(table)
    }

    async fn table_commit_records(
        &self,
        ident: &TableIdent,
        start_version: u64,
        end_version: Option<u64>,
    ) -> LakeCatResult<Vec<TableCommitRecord>> {
        let state = self.state.read().await;
        Ok(state
            .commits
            .iter()
            .filter(|commit| &commit.table == ident)
            .filter(|commit| commit.sequence_number >= start_version)
            .filter(|commit| end_version.is_none_or(|end| commit.sequence_number <= end))
            .cloned()
            .collect())
    }

    async fn upsert_server(&self, server: ServerRecord) -> LakeCatResult<ServerRecord> {
        server.validate()?;
        let mut state = self.state.write().await;
        state
            .servers
            .insert(server.server_id.clone(), server.clone());
        Ok(server)
    }

    async fn list_servers(&self) -> LakeCatResult<Vec<ServerRecord>> {
        let state = self.state.read().await;
        let mut servers = state.servers.values().cloned().collect::<Vec<_>>();
        servers.sort_by(|left, right| left.server_id.cmp(&right.server_id));
        Ok(servers)
    }

    async fn upsert_project(&self, project: ProjectRecord) -> LakeCatResult<ProjectRecord> {
        project.validate()?;
        let mut state = self.state.write().await;
        if let Some(server_id) = project.server_id.as_deref()
            && !state.servers.contains_key(server_id)
        {
            return Err(LakeCatError::NotFound {
                object: "server",
                name: server_id.to_string(),
            });
        }
        state
            .projects
            .insert(project.project_id.clone(), project.clone());
        Ok(project)
    }

    async fn list_projects(&self) -> LakeCatResult<Vec<ProjectRecord>> {
        let state = self.state.read().await;
        let mut projects = state.projects.values().cloned().collect::<Vec<_>>();
        projects.sort_by(|left, right| left.project_id.cmp(&right.project_id));
        Ok(projects)
    }

    async fn upsert_warehouse(&self, warehouse: WarehouseRecord) -> LakeCatResult<WarehouseRecord> {
        warehouse.validate()?;
        let mut state = self.state.write().await;
        if !state.projects.contains_key(&warehouse.project_id) {
            return Err(LakeCatError::NotFound {
                object: "project",
                name: warehouse.project_id.clone(),
            });
        }
        state
            .warehouses
            .insert(warehouse.warehouse.as_str().to_string(), warehouse.clone());
        Ok(warehouse)
    }

    async fn load_warehouse(&self, warehouse: &WarehouseName) -> LakeCatResult<WarehouseRecord> {
        let state = self.state.read().await;
        state
            .warehouses
            .get(warehouse.as_str())
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "warehouse",
                name: warehouse.as_str().to_string(),
            })
    }

    async fn list_warehouses(&self) -> LakeCatResult<Vec<WarehouseRecord>> {
        let state = self.state.read().await;
        let mut warehouses = state.warehouses.values().cloned().collect::<Vec<_>>();
        warehouses.sort_by(|left, right| left.warehouse.as_str().cmp(right.warehouse.as_str()));
        Ok(warehouses)
    }

    async fn list_project_warehouses(
        &self,
        project_id: &str,
    ) -> LakeCatResult<Vec<WarehouseRecord>> {
        validate_project_id(project_id)?;
        let state = self.state.read().await;
        if !state.projects.contains_key(project_id) {
            return Err(LakeCatError::NotFound {
                object: "project",
                name: project_id.to_string(),
            });
        }
        let mut warehouses = state
            .warehouses
            .values()
            .filter(|warehouse| warehouse.project_id == project_id)
            .cloned()
            .collect::<Vec<_>>();
        warehouses.sort_by(|left, right| left.warehouse.as_str().cmp(right.warehouse.as_str()));
        Ok(warehouses)
    }

    async fn soft_delete_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord> {
        let mut state = self.state.write().await;
        let key = table_key(ident);
        if state.soft_deletes.contains_key(&key) {
            return Err(LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            });
        }
        let table = state
            .tables
            .get(&key)
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })?;
        state.soft_deletes.insert(
            key,
            SoftDeleteRecord {
                table: ident.clone(),
                metadata_location: table.metadata_location.clone(),
                version: table.version,
                principal,
                authorization_receipt,
                deleted_at: Utc::now(),
            },
        );
        Ok(table)
    }

    async fn restore_table(
        &self,
        ident: &TableIdent,
        _principal: Principal,
        _authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord> {
        let mut state = self.state.write().await;
        let key = table_key(ident);
        if state.soft_deletes.remove(&key).is_none() {
            return Err(LakeCatError::NotFound {
                object: "soft-deleted table",
                name: ident.stable_id(),
            });
        }
        state
            .tables
            .get(&key)
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })
    }

    async fn upsert_storage_profile(
        &self,
        profile: StorageProfile,
    ) -> LakeCatResult<StorageProfile> {
        profile.validate()?;
        let mut state = self.state.write().await;
        state.storage_profiles.insert(
            storage_profile_key(&profile.warehouse, &profile.profile_id),
            profile.clone(),
        );
        Ok(profile)
    }

    async fn list_storage_profiles(
        &self,
        warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<StorageProfile>> {
        let state = self.state.read().await;
        let mut profiles = state
            .storage_profiles
            .values()
            .filter(|profile| profile.warehouse == *warehouse)
            .cloned()
            .collect::<Vec<_>>();
        profiles.sort_by(|left, right| left.profile_id.cmp(&right.profile_id));
        Ok(profiles)
    }

    async fn upsert_view(&self, view: ViewRecord) -> LakeCatResult<ViewRecord> {
        view.validate()?;
        let mut state = self.state.write().await;
        state.views.insert(view_key(&view), view.clone());
        Ok(view)
    }

    async fn load_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<ViewRecord> {
        let state = self.state.read().await;
        state
            .views
            .get(&view_key_parts(warehouse, namespace, view))
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "view",
                name: view.as_str().to_string(),
            })
    }

    async fn drop_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<ViewRecord> {
        let mut state = self.state.write().await;
        state
            .views
            .remove(&view_key_parts(warehouse, namespace, view))
            .ok_or_else(|| LakeCatError::NotFound {
                object: "view",
                name: view.as_str().to_string(),
            })
    }

    async fn list_views(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewRecord>> {
        let state = self.state.read().await;
        let mut views = state
            .views
            .values()
            .filter(|view| view.warehouse == *warehouse && view.namespace == *namespace)
            .cloned()
            .collect::<Vec<_>>();
        views.sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
        Ok(views)
    }

    async fn storage_profile_for_table(
        &self,
        table: &TableRecord,
    ) -> LakeCatResult<StorageProfile> {
        let state = self.state.read().await;
        Ok(
            storage_profile_match(state.storage_profiles.values(), table)
                .unwrap_or_else(|| StorageProfile::inferred_for_table(table)),
        )
    }

    async fn upsert_policy_binding(&self, binding: PolicyBinding) -> LakeCatResult<PolicyBinding> {
        let mut state = self.state.write().await;
        state.policy_bindings.insert(
            policy_binding_key(&binding.warehouse, &binding.policy_id),
            binding.clone(),
        );
        Ok(binding)
    }

    async fn list_policy_bindings(
        &self,
        warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        let state = self.state.read().await;
        let mut bindings = state
            .policy_bindings
            .values()
            .filter(|binding| binding.warehouse == *warehouse)
            .cloned()
            .collect::<Vec<_>>();
        bindings.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));
        Ok(bindings)
    }

    async fn policy_bindings_for_table(
        &self,
        table: &TableIdent,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        let state = self.state.read().await;
        Ok(policy_bindings_for_table(
            state.policy_bindings.values(),
            table,
        ))
    }

    async fn record_audit_event(&self, event: CatalogAuditEvent) -> LakeCatResult<()> {
        let event_id = audit_event_id(&event)?;
        let outbox_payload = audit_outbox_payload(&event_id, &event);
        let outbox_event = outbox_event_from_payload(&outbox_payload, event.created_at)?;
        let mut state = self.state.write().await;
        state.audit_events.push(event);
        state.outbox_events.push(outbox_event);
        Ok(())
    }

    async fn pending_outbox_events(
        &self,
        sink: Option<&str>,
        limit: usize,
    ) -> LakeCatResult<Vec<OutboxEvent>> {
        let state = self.state.read().await;
        Ok(state
            .outbox_events
            .iter()
            .filter(|event| event.delivered_at.is_none())
            .filter(|event| sink.is_none_or(|sink| event.sink == sink))
            .take(limit)
            .cloned()
            .collect())
    }

    async fn mark_outbox_delivered(&self, event_ids: &[String]) -> LakeCatResult<usize> {
        if event_ids.is_empty() {
            return Ok(0);
        }
        let mut state = self.state.write().await;
        let delivered_at = Utc::now();
        let mut delivered = 0usize;
        for event in &mut state.outbox_events {
            if event.delivered_at.is_none()
                && event_ids.iter().any(|event_id| event_id == &event.event_id)
            {
                event.delivered_at = Some(delivered_at);
                delivered += 1;
            }
        }
        Ok(delivered)
    }
}

pub fn table_ident(
    warehouse: impl Into<String>,
    namespace: impl AsRef<str>,
    table: impl Into<String>,
) -> LakeCatResult<TableIdent> {
    Ok(TableIdent::new(
        WarehouseName::new(warehouse.into())?,
        namespace.as_ref().parse()?,
        TableName::new(table.into())?,
    ))
}

fn table_key(ident: &TableIdent) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        ident.warehouse, ident.namespace, ident.name
    )
}

fn view_key(view: &ViewRecord) -> String {
    view_key_parts(&view.warehouse, &view.namespace, &view.name)
}

fn view_key_parts(warehouse: &WarehouseName, namespace: &Namespace, name: &TableName) -> String {
    format!("{warehouse}\u{1f}{namespace}\u{1f}{name}")
}

fn audit_event_id(event: &CatalogAuditEvent) -> LakeCatResult<String> {
    content_hash_json(&serde_json::json!({
        "event-type": &event.event_type,
        "table": &event.table,
        "principal": &event.principal,
        "request-hash": &event.request_hash,
        "payload": &event.payload,
        "created-at": event.created_at.to_rfc3339(),
    }))
}

fn audit_outbox_payload(event_id: &str, event: &CatalogAuditEvent) -> Value {
    serde_json::json!({
        "audit-event-id": event_id,
        "event-type": &event.event_type,
        "table": &event.table,
        "payload": &event.payload,
    })
}

fn outbox_event_from_payload(
    payload: &Value,
    created_at: DateTime<Utc>,
) -> LakeCatResult<OutboxEvent> {
    let event_type = payload["event-type"]
        .as_str()
        .ok_or_else(|| LakeCatError::Internal("outbox payload missing event-type".to_string()))?;
    Ok(OutboxEvent {
        event_id: content_hash_json(payload)?,
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: event_type.to_string(),
        payload: payload.clone(),
        created_at,
        delivered_at: None,
    })
}

fn storage_profile_key(warehouse: &WarehouseName, profile_id: &str) -> String {
    format!("{}:{profile_id}", warehouse.as_str())
}

fn policy_binding_key(warehouse: &WarehouseName, policy_id: &str) -> String {
    format!("{}:{policy_id}", warehouse.as_str())
}

fn storage_profile_match<'a>(
    profiles: impl IntoIterator<Item = &'a StorageProfile>,
    table: &TableRecord,
) -> Option<StorageProfile> {
    profiles
        .into_iter()
        .filter(|profile| profile.warehouse == table.ident.warehouse)
        .filter(|profile| table.location.starts_with(&profile.location_prefix))
        .max_by_key(|profile| profile.location_prefix.len())
        .cloned()
}

fn validate_profile_id(profile_id: &str) -> LakeCatResult<()> {
    if profile_id.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "storage profile id must not be empty".to_string(),
        ));
    }
    if !profile_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile id contains unsupported characters: {profile_id}"
        )));
    }
    Ok(())
}

fn validate_project_id(project_id: &str) -> LakeCatResult<()> {
    if project_id.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "project id must not be empty".to_string(),
        ));
    }
    if !project_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "project id contains unsupported characters: {project_id}"
        )));
    }
    Ok(())
}

fn validate_public_config(config: &BTreeMap<String, String>) -> LakeCatResult<()> {
    for key in config.keys() {
        let normalized = key.to_ascii_lowercase();
        if normalized.contains("secret")
            || normalized.contains("token")
            || normalized.contains("password")
            || normalized.contains("credential")
        {
            return Err(LakeCatError::InvalidArgument(format!(
                "storage profile public config key may expose secret material: {key}"
            )));
        }
    }
    Ok(())
}

fn validate_secret_ref(secret_ref: &str) -> LakeCatResult<()> {
    let trimmed = secret_ref.trim();
    if trimmed.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "storage profile secret reference must not be empty".to_string(),
        ));
    }
    let allowed = [
        "typesec://",
        "vault://",
        "aws-sm://",
        "gcp-sm://",
        "azure-kv://",
    ];
    if !allowed.iter().any(|prefix| trimmed.starts_with(prefix)) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must use an external secret-store URI: {secret_ref}"
        )));
    }
    let normalized = trimmed.to_ascii_lowercase();
    let embedded_secret_patterns = [
        "password=",
        "secret=",
        "token=",
        "credential=",
        "api_key=",
        "apikey=",
        "access_key=",
        "private_key=",
        "pass=",
        "auth=",
    ];
    if embedded_secret_patterns
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return Err(LakeCatError::InvalidArgument(
            "storage profile secret reference must not embed raw secret material".to_string(),
        ));
    }
    Ok(())
}

fn validate_policy_id(policy_id: &str) -> LakeCatResult<()> {
    if policy_id.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "policy id must not be empty".to_string(),
        ));
    }
    if !policy_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "policy id contains unsupported characters: {policy_id}"
        )));
    }
    Ok(())
}

fn policy_bindings_for_table<'a>(
    bindings: impl IntoIterator<Item = &'a PolicyBinding>,
    table: &TableIdent,
) -> Vec<PolicyBinding> {
    let mut bindings = bindings
        .into_iter()
        .filter(|binding| binding.enforced && binding.applies_to_table(table))
        .cloned()
        .collect::<Vec<_>>();
    bindings.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));
    bindings
}

#[cfg(test)]
mod memory_tests {
    use std::collections::BTreeMap;

    use lakecat_core::{Principal, TableName};

    use super::*;

    #[tokio::test]
    async fn memory_store_persists_server_records() {
        let store = MemoryCatalogStore::new();
        assert_eq!(store.list_servers().await.unwrap(), vec![]);

        let record = ServerRecord::new(
            "lakecat-local",
            Some("Local LakeCat".to_string()),
            Some("http://127.0.0.1:8181".to_string()),
            BTreeMap::from([("deployment".to_string(), "local".to_string())]),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_server(record).await.unwrap();

        let updated = ServerRecord::new(
            "lakecat-local",
            Some("Local QueryGraph LakeCat".to_string()),
            Some("http://127.0.0.1:8182".to_string()),
            BTreeMap::from([("deployment".to_string(), "dev".to_string())]),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_server(updated.clone()).await.unwrap();

        assert_eq!(store.list_servers().await.unwrap(), vec![updated]);
    }

    #[tokio::test]
    async fn memory_store_persists_warehouse_records() {
        let store = MemoryCatalogStore::new();
        assert_eq!(store.list_warehouses().await.unwrap(), vec![]);
        let project = ProjectRecord::new(
            "default",
            None,
            Some("Default Project".to_string()),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_project(project).await.unwrap();

        let warehouse = WarehouseName::new("local").unwrap();
        let record = WarehouseRecord::new(
            warehouse.clone(),
            "default",
            Some("file:///tmp/lakecat".to_string()),
            BTreeMap::from([("region".to_string(), "local".to_string())]),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_warehouse(record).await.unwrap();

        let updated = WarehouseRecord::new(
            warehouse.clone(),
            "default",
            Some("file:///tmp/lakecat-updated".to_string()),
            BTreeMap::from([("region".to_string(), "test".to_string())]),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_warehouse(updated.clone()).await.unwrap();

        assert_eq!(store.load_warehouse(&warehouse).await.unwrap(), updated);
        assert!(matches!(
            store
                .load_warehouse(&WarehouseName::new("missing").unwrap())
                .await,
            Err(LakeCatError::NotFound { object, name })
                if object == "warehouse" && name == "missing"
        ));
        assert_eq!(
            store.list_warehouses().await.unwrap(),
            vec![updated.clone()]
        );
        assert_eq!(
            store.list_project_warehouses("default").await.unwrap(),
            vec![updated.clone()]
        );
        assert!(matches!(
            store.list_project_warehouses("missing-project").await,
            Err(LakeCatError::NotFound { object, name })
                if object == "project" && name == "missing-project"
        ));

        let missing_project = WarehouseRecord::new(
            WarehouseName::new("orphaned").unwrap(),
            "missing-project",
            Some("file:///tmp/orphaned".to_string()),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        assert!(matches!(
            store.upsert_warehouse(missing_project).await,
            Err(LakeCatError::NotFound { object, name })
                if object == "project" && name == "missing-project"
        ));
    }

    #[tokio::test]
    async fn memory_store_persists_project_records() {
        let store = MemoryCatalogStore::new();
        assert_eq!(store.list_projects().await.unwrap(), vec![]);

        let record = ProjectRecord::new(
            "default",
            Some("lakecat-local".to_string()),
            Some("Default Project".to_string()),
            BTreeMap::from([("owner".to_string(), "querygraph".to_string())]),
            Principal::anonymous(),
        )
        .unwrap();
        store
            .upsert_server(
                ServerRecord::new(
                    "lakecat-local",
                    Some("Local LakeCat".to_string()),
                    None,
                    BTreeMap::new(),
                    Principal::anonymous(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        store.upsert_project(record).await.unwrap();

        let updated = ProjectRecord::new(
            "default",
            Some("lakecat-local".to_string()),
            Some("QueryGraph Project".to_string()),
            BTreeMap::from([("owner".to_string(), "lakecat".to_string())]),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_project(updated.clone()).await.unwrap();

        assert_eq!(store.list_projects().await.unwrap(), vec![updated]);

        let missing_server = ProjectRecord::new(
            "orphaned",
            Some("missing-server".to_string()),
            Some("Orphaned Project".to_string()),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        assert!(matches!(
            store.upsert_project(missing_server).await,
            Err(LakeCatError::NotFound { object, name })
                if object == "server" && name == "missing-server"
        ));
    }

    #[tokio::test]
    async fn memory_store_persists_view_records() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        assert_eq!(
            store.list_views(&warehouse, &namespace).await.unwrap(),
            vec![]
        );

        let view = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("active_customers").unwrap(),
            "select * from customers where active",
            "sql",
            Some(1),
            BTreeMap::from([("owner".to_string(), "querygraph".to_string())]),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_view(view).await.unwrap();

        let updated = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("active_customers").unwrap(),
            "select id, email from customers where active",
            "sql",
            Some(2),
            BTreeMap::from([("owner".to_string(), "lakecat".to_string())]),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_view(updated.clone()).await.unwrap();

        assert_eq!(
            store
                .load_view(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap()
                )
                .await
                .unwrap(),
            updated.clone()
        );
        assert!(matches!(
            store
                .load_view(&warehouse, &namespace, &TableName::new("missing_view").unwrap())
                .await,
            Err(LakeCatError::NotFound { object, name })
                if object == "view" && name == "missing_view"
        ));
        assert_eq!(
            store.list_views(&warehouse, &namespace).await.unwrap(),
            vec![updated.clone()]
        );
        assert_eq!(
            store
                .drop_view(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap()
                )
                .await
                .unwrap(),
            updated
        );
        assert_eq!(
            store.list_views(&warehouse, &namespace).await.unwrap(),
            Vec::<ViewRecord>::new()
        );
        assert!(matches!(
            store
                .drop_view(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap()
                )
                .await,
            Err(LakeCatError::NotFound { object, name })
                if object == "view" && name == "active_customers"
        ));
    }

    #[tokio::test]
    async fn memory_store_records_and_marks_audit_outbox_events() {
        let store = MemoryCatalogStore::new();
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        store
            .record_audit_event(
                CatalogAuditEvent::new(
                    "querygraph.bootstrap",
                    Some(ident.clone()),
                    Principal::anonymous(),
                    serde_json::json!({
                        "event-type": "querygraph.bootstrap",
                        "table": ident,
                        "authorization-receipt": {
                            "engine": "typesec",
                            "allowed": true,
                            "action": "querygraph.bootstrap"
                        },
                        "manifest-hash": "lakecat:test"
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let pending = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].sink, "lakecat.lineage-and-graph");
        assert_eq!(pending[0].event_type, "querygraph.bootstrap");
        assert_eq!(
            pending[0].payload["payload"]["authorization-receipt"]["engine"],
            serde_json::json!("typesec")
        );
        assert_eq!(
            pending[0].payload["payload"]["manifest-hash"],
            serde_json::json!("lakecat:test")
        );

        let unrelated = store
            .pending_outbox_events(Some("lakecat.unrelated"), 10)
            .await
            .unwrap();
        assert!(unrelated.is_empty());

        let event_ids = pending
            .iter()
            .map(|event| event.event_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(store.mark_outbox_delivered(&event_ids).await.unwrap(), 1);
        assert!(
            store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(store.mark_outbox_delivered(&event_ids).await.unwrap(), 0);
    }
}

#[cfg(feature = "turso-local")]
pub mod turso_store {
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;
    use lakecat_core::{
        LakeCatError, LakeCatResult, Namespace, TableIdent, TableName, WarehouseName,
        content_hash_bytes, content_hash_json,
    };
    use serde::de::DeserializeOwned;
    use serde_json::Value as JsonValue;
    use turso::{Connection, Database, Row, Value as TursoValue};

    use crate::{
        CatalogAuditEvent, CatalogStore, OutboxEvent, PolicyBinding, ProjectRecord, ServerRecord,
        SoftDeleteRecord, StorageProfile, TableCommit, TableCommitRecord, TableRecord, ViewRecord,
        WarehouseRecord, policy_binding_key, policy_bindings_for_table, storage_profile_key,
        storage_profile_match, table_key, validate_project_id, view_key, view_key_parts,
    };

    #[derive(Debug, Clone)]
    pub struct TursoCatalogStore {
        db: Database,
    }

    impl TursoCatalogStore {
        pub async fn connect_local(path: &str) -> LakeCatResult<Arc<Self>> {
            let db = turso::Builder::new_local(path)
                .build()
                .await
                .map_err(turso_error)?;
            Self::from_database(db).await
        }

        pub async fn in_memory() -> LakeCatResult<Arc<Self>> {
            let db = turso::Builder::new_local(":memory:")
                .build()
                .await
                .map_err(turso_error)?;
            Self::from_database(db).await
        }

        pub async fn from_database(db: Database) -> LakeCatResult<Arc<Self>> {
            let store = Arc::new(Self { db });
            store.migrate().await?;
            Ok(store)
        }

        pub fn database(&self) -> &Database {
            &self.db
        }

        async fn migrate(&self) -> LakeCatResult<()> {
            let conn = self.connect()?;
            conn.execute_batch(TURSO_MIGRATION.join(";\n"))
                .await
                .map_err(turso_error)?;
            Ok(())
        }

        fn connect(&self) -> LakeCatResult<Connection> {
            self.db.connect().map_err(turso_error)
        }

        #[cfg(test)]
        async fn count_rows(&self, table: &str) -> LakeCatResult<i64> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(format!("select count(*) from {table}"), ())
                .await
                .map_err(turso_error)?;
            let row = rows.next().await.map_err(turso_error)?.ok_or_else(|| {
                LakeCatError::Internal(format!("Turso catalog store returned no count for {table}"))
            })?;
            row_i64(&row, 0)
        }
    }

    #[async_trait]
    impl CatalogStore for TursoCatalogStore {
        async fn create_namespace(
            &self,
            warehouse: &WarehouseName,
            namespace: Namespace,
        ) -> LakeCatResult<()> {
            let conn = self.connect()?;
            conn.execute(
                "insert or ignore into namespaces (warehouse, namespace_path, namespace_json)
                 values (?1, ?2, ?3)",
                (
                    warehouse.as_str(),
                    namespace.path(),
                    encode_json(namespace.parts())?,
                ),
            )
            .await
            .map_err(turso_error)?;
            Ok(())
        }

        async fn list_namespaces(
            &self,
            warehouse: &WarehouseName,
        ) -> LakeCatResult<Vec<Namespace>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select namespace_json from namespaces
                     where warehouse = ?1
                     order by namespace_path",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut namespaces = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                namespaces.push(decode_namespace(row_string(&row, 0)?)?);
            }
            Ok(namespaces)
        }

        async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json from tables t
                     where warehouse = ?1
                       and not exists (
                         select 1 from soft_deletes d where d.table_key = t.table_key
                       )
                     order by namespace_path, table_name",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut tables = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                tables.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(tables)
        }

        async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord> {
            let mut conn = self.connect()?;
            let tx = conn.transaction().await.map_err(turso_error)?;
            tx.execute(
                "insert or ignore into namespaces (warehouse, namespace_path, namespace_json)
                 values (?1, ?2, ?3)",
                (
                    table.ident.warehouse.as_str(),
                    table.ident.namespace.path(),
                    encode_json(table.ident.namespace.parts())?,
                ),
            )
            .await
            .map_err(turso_error)?;

            let result = tx
                .execute(
                    "insert into tables (
                    table_key, warehouse, namespace_path, table_name,
                    metadata_location, version, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    (
                        table_key(&table.ident),
                        table.ident.warehouse.as_str(),
                        table.ident.namespace.path(),
                        table.ident.name.as_str(),
                        table.metadata_location.as_deref(),
                        checked_i64(table.version, "table version")?,
                        encode_json(&table)?,
                        table.updated_at.to_rfc3339(),
                    ),
                )
                .await;

            match result {
                Ok(_) => {
                    tx.commit().await.map_err(turso_error)?;
                    Ok(table)
                }
                Err(err) if is_unique_violation(&err) => Err(LakeCatError::Conflict(format!(
                    "table already exists: {}",
                    table.ident.stable_id()
                ))),
                Err(err) => Err(turso_error(err)),
            }
        }

        async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json from tables t
                     where t.table_key = ?1
                       and not exists (
                         select 1 from soft_deletes d where d.table_key = t.table_key
                       )",
                    (table_key(ident),),
                )
                .await
                .map_err(turso_error)?;
            rows.next()
                .await
                .map_err(turso_error)?
                .map(|row| decode_json(row_string(&row, 0)?))
                .transpose()?
                .ok_or_else(|| LakeCatError::NotFound {
                    object: "table",
                    name: ident.stable_id(),
                })
        }

        async fn commit_table(
            &self,
            ident: &TableIdent,
            commit: TableCommit,
        ) -> LakeCatResult<TableRecord> {
            let mut conn = self.connect()?;
            let tx = conn.transaction().await.map_err(turso_error)?;
            let request_hash = content_hash_json(&serde_json::json!({
                "requirements": &commit.requirements,
                "updates": &commit.updates,
                "expected_previous_metadata_location": &commit.expected_previous_metadata_location,
                "new_metadata_location": &commit.new_metadata_location,
                "new_metadata": &commit.new_metadata,
            }))?;
            let idempotency_request_hash = commit
                .idempotency_request_hash
                .clone()
                .unwrap_or_else(|| request_hash.clone());
            if let Some(idempotency_key) = &commit.idempotency_key {
                let idem_key = idempotency_record_key(ident, idempotency_key);
                let mut rows = tx
                    .query(
                        "select request_hash, response_json from idempotency_records where idem_key = ?1",
                        (idem_key,),
                    )
                    .await
                    .map_err(turso_error)?;
                if let Some(row) = rows.next().await.map_err(turso_error)? {
                    let replay_hash = row_string(&row, 0)?;
                    if replay_hash != idempotency_request_hash {
                        return Err(LakeCatError::Conflict(format!(
                            "idempotency key reused with different commit request for {}",
                            ident.stable_id()
                        )));
                    }
                    let table = decode_json(row_string(&row, 1)?)?;
                    tx.commit().await.map_err(turso_error)?;
                    return Ok(table);
                }
            }

            let mut rows = tx
                .query(
                    "select record_json from tables t
                     where t.table_key = ?1
                       and not exists (
                         select 1 from soft_deletes d where d.table_key = t.table_key
                       )",
                    (table_key(ident),),
                )
                .await
                .map_err(turso_error)?;
            let Some(row) = rows.next().await.map_err(turso_error)? else {
                return Err(LakeCatError::NotFound {
                    object: "table",
                    name: ident.stable_id(),
                });
            };
            let mut table: TableRecord = decode_json(row_string(&row, 0)?)?;
            let previous_metadata_location = table.metadata_location.clone();
            let idempotency_key_sha256 = commit
                .idempotency_key
                .as_ref()
                .map(|key| content_hash_bytes(key.as_bytes()));
            if previous_metadata_location != commit.expected_previous_metadata_location {
                return Err(LakeCatError::Conflict(format!(
                    "metadata pointer changed for {}",
                    ident.stable_id()
                )));
            }
            table.metadata_location = commit.new_metadata_location.clone();
            if let Some(new_metadata) = commit.new_metadata {
                table.metadata = new_metadata;
            }
            table.version += 1;
            table.updated_at = Utc::now();
            table.metadata["lakecat:version"] = serde_json::json!(table.version);
            table.metadata["lakecat:last-request-hash"] = serde_json::json!(request_hash);

            let updated_rows = tx
                .execute(
                    "update tables
                 set metadata_location = ?2, version = ?3, record_json = ?4, updated_at = ?5
                 where table_key = ?1
                   and (
                     (metadata_location is null and ?6 is null)
                     or metadata_location = ?7
                   )",
                    (
                        table_key(ident),
                        table.metadata_location.as_deref(),
                        checked_i64(table.version, "table version")?,
                        encode_json(&table)?,
                        table.updated_at.to_rfc3339(),
                        commit.expected_previous_metadata_location.as_deref(),
                        commit.expected_previous_metadata_location.as_deref(),
                    ),
                )
                .await
                .map_err(turso_error)?;
            if updated_rows == 0 {
                return Err(LakeCatError::Conflict(format!(
                    "metadata pointer changed for {}",
                    ident.stable_id()
                )));
            }

            let record = TableCommitRecord {
                table: ident.clone(),
                previous_metadata_location,
                new_metadata_location: table.metadata_location.clone(),
                sequence_number: table.version,
                principal: commit.principal.clone(),
                request_hash,
                idempotency_key_sha256,
                committed_at: table.updated_at,
            };
            tx.execute(
                "insert into metadata_pointer_log (
                    table_key, sequence_number, previous_metadata_location,
                    new_metadata_location, principal_json, request_hash,
                    committed_at, record_json
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                (
                    table_key(ident),
                    checked_i64(record.sequence_number, "sequence number")?,
                    record.previous_metadata_location.as_deref(),
                    record.new_metadata_location.as_deref(),
                    encode_json(&record.principal)?,
                    record.request_hash.as_str(),
                    record.committed_at.to_rfc3339(),
                    encode_json(&record)?,
                ),
            )
            .await
            .map_err(turso_error)?;

            let audit_payload = serde_json::json!({
                "event-type": "table.commit",
                "table": ident,
                "commit": record,
                "authorization-receipt": commit.authorization_receipt,
            });
            let audit_event_id = content_hash_json(&audit_payload)?;
            tx.execute(
                "insert into audit_events (
                    event_id, event_type, table_key, principal_json,
                    request_hash, event_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                (
                    audit_event_id.as_str(),
                    "table.commit",
                    table_key(ident),
                    encode_json(&commit.principal)?,
                    record.request_hash.as_str(),
                    encode_json(&audit_payload)?,
                    table.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;

            let outbox_payload = serde_json::json!({
                "audit-event-id": audit_event_id,
                "event-type": "table.commit",
                "table": ident,
                "commit": record,
                "authorization-receipt": audit_payload["authorization-receipt"].clone(),
            });
            tx.execute(
                "insert into outbox_events (
                    event_id, sink, event_type, payload_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)",
                (
                    content_hash_json(&outbox_payload)?,
                    "lakecat.lineage-and-graph",
                    "table.commit",
                    encode_json(&outbox_payload)?,
                    table.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;

            if let Some(idempotency_key) = commit.idempotency_key {
                tx.execute(
                    "insert into idempotency_records (
                        idem_key, table_key, request_hash, response_json, created_at
                     )
                     values (?1, ?2, ?3, ?4, ?5)",
                    (
                        idempotency_record_key(ident, &idempotency_key),
                        table_key(ident),
                        commit
                            .idempotency_request_hash
                            .as_deref()
                            .unwrap_or(record.request_hash.as_str()),
                        encode_json(&table)?,
                        table.updated_at.to_rfc3339(),
                    ),
                )
                .await
                .map_err(turso_error)?;
            }

            tx.commit().await.map_err(turso_error)?;
            Ok(table)
        }

        async fn soft_delete_table(
            &self,
            ident: &TableIdent,
            principal: lakecat_core::Principal,
            authorization_receipt: Option<JsonValue>,
        ) -> LakeCatResult<TableRecord> {
            let mut conn = self.connect()?;
            let tx = conn.transaction().await.map_err(turso_error)?;
            let mut rows = tx
                .query(
                    "select record_json from tables t
                     where t.table_key = ?1
                       and not exists (
                         select 1 from soft_deletes d where d.table_key = t.table_key
                       )",
                    (table_key(ident),),
                )
                .await
                .map_err(turso_error)?;
            let Some(row) = rows.next().await.map_err(turso_error)? else {
                return Err(LakeCatError::NotFound {
                    object: "table",
                    name: ident.stable_id(),
                });
            };
            let table: TableRecord = decode_json(row_string(&row, 0)?)?;
            let deleted_at = Utc::now();
            let record = SoftDeleteRecord {
                table: ident.clone(),
                metadata_location: table.metadata_location.clone(),
                version: table.version,
                principal: principal.clone(),
                authorization_receipt,
                deleted_at,
            };
            tx.execute(
                "insert into soft_deletes (
                    table_key, warehouse, namespace_path, table_name,
                    metadata_location, version, principal_json,
                    authorization_receipt_json, record_json, deleted_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                (
                    table_key(ident),
                    ident.warehouse.as_str(),
                    ident.namespace.path(),
                    ident.name.as_str(),
                    table.metadata_location.as_deref(),
                    checked_i64(table.version, "table version")?,
                    encode_json(&principal)?,
                    record
                        .authorization_receipt
                        .as_ref()
                        .map(encode_json)
                        .transpose()?,
                    encode_json(&record)?,
                    deleted_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;

            let audit_payload = serde_json::json!({
                "event-type": "table.deleted",
                "table": ident,
                "soft-delete": record,
                "authorization-receipt": record.authorization_receipt,
            });
            let audit_event_id = content_hash_json(&audit_payload)?;
            tx.execute(
                "insert into audit_events (
                    event_id, event_type, table_key, principal_json,
                    request_hash, event_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                (
                    audit_event_id.as_str(),
                    "table.deleted",
                    table_key(ident),
                    encode_json(&principal)?,
                    audit_event_id.as_str(),
                    encode_json(&audit_payload)?,
                    deleted_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            let outbox_payload = serde_json::json!({
                "audit-event-id": audit_event_id,
                "event-type": "table.deleted",
                "table": ident,
                "soft-delete": audit_payload["soft-delete"].clone(),
                "authorization-receipt": audit_payload["authorization-receipt"].clone(),
            });
            tx.execute(
                "insert into outbox_events (
                    event_id, sink, event_type, payload_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)",
                (
                    content_hash_json(&outbox_payload)?,
                    "lakecat.lineage-and-graph",
                    "table.deleted",
                    encode_json(&outbox_payload)?,
                    deleted_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            tx.commit().await.map_err(turso_error)?;
            Ok(table)
        }

        async fn restore_table(
            &self,
            ident: &TableIdent,
            principal: lakecat_core::Principal,
            authorization_receipt: Option<JsonValue>,
        ) -> LakeCatResult<TableRecord> {
            let mut conn = self.connect()?;
            let tx = conn.transaction().await.map_err(turso_error)?;
            let mut rows = tx
                .query(
                    "select t.record_json from tables t
                     join soft_deletes d on d.table_key = t.table_key
                     where t.table_key = ?1",
                    (table_key(ident),),
                )
                .await
                .map_err(turso_error)?;
            let Some(row) = rows.next().await.map_err(turso_error)? else {
                return Err(LakeCatError::NotFound {
                    object: "soft-deleted table",
                    name: ident.stable_id(),
                });
            };
            let table: TableRecord = decode_json(row_string(&row, 0)?)?;
            let restored_at = Utc::now();
            let changed = tx
                .execute(
                    "delete from soft_deletes where table_key = ?1",
                    (table_key(ident),),
                )
                .await
                .map_err(turso_error)?;
            if changed == 0 {
                return Err(LakeCatError::NotFound {
                    object: "soft-deleted table",
                    name: ident.stable_id(),
                });
            }

            let audit_payload = serde_json::json!({
                "event-type": "table.restored",
                "table": ident,
                "authorization-receipt": authorization_receipt,
                "metadata-location": table.metadata_location,
                "version": table.version,
            });
            let audit_event_id = content_hash_json(&audit_payload)?;
            tx.execute(
                "insert into audit_events (
                    event_id, event_type, table_key, principal_json,
                    request_hash, event_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                (
                    audit_event_id.as_str(),
                    "table.restored",
                    table_key(ident),
                    encode_json(&principal)?,
                    audit_event_id.as_str(),
                    encode_json(&audit_payload)?,
                    restored_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            let outbox_payload = serde_json::json!({
                "audit-event-id": audit_event_id,
                "event-type": "table.restored",
                "table": ident,
                "payload": audit_payload,
                "authorization-receipt": audit_payload["authorization-receipt"].clone(),
            });
            tx.execute(
                "insert into outbox_events (
                    event_id, sink, event_type, payload_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)",
                (
                    content_hash_json(&outbox_payload)?,
                    "lakecat.lineage-and-graph",
                    "table.restored",
                    encode_json(&outbox_payload)?,
                    restored_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            tx.commit().await.map_err(turso_error)?;
            Ok(table)
        }

        async fn table_commit_records(
            &self,
            ident: &TableIdent,
            start_version: u64,
            end_version: Option<u64>,
        ) -> LakeCatResult<Vec<TableCommitRecord>> {
            let conn = self.connect()?;
            let end_version = end_version.unwrap_or(i64::MAX as u64);
            let mut rows = conn
                .query(
                    "select record_json from metadata_pointer_log
                     where table_key = ?1
                       and sequence_number >= ?2
                       and sequence_number <= ?3
                     order by sequence_number",
                    (
                        table_key(ident),
                        checked_i64(start_version, "start version")?,
                        checked_i64(end_version, "end version")?,
                    ),
                )
                .await
                .map_err(turso_error)?;
            let mut commits = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                commits.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(commits)
        }

        async fn upsert_server(&self, server: ServerRecord) -> LakeCatResult<ServerRecord> {
            server.validate()?;
            let conn = self.connect()?;
            conn.execute(
                "insert into servers (
                    server_id, display_name, endpoint_url, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)
                 on conflict(server_id) do update set
                    display_name = excluded.display_name,
                    endpoint_url = excluded.endpoint_url,
                    record_json = excluded.record_json,
                    updated_at = excluded.updated_at",
                (
                    server.server_id.as_str(),
                    server.display_name.as_deref(),
                    server.endpoint_url.as_deref(),
                    encode_json(&server)?,
                    server.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            Ok(server)
        }

        async fn list_servers(&self) -> LakeCatResult<Vec<ServerRecord>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json from servers
                     order by server_id",
                    (),
                )
                .await
                .map_err(turso_error)?;
            let mut servers = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                servers.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(servers)
        }

        async fn upsert_project(&self, project: ProjectRecord) -> LakeCatResult<ProjectRecord> {
            project.validate()?;
            let conn = self.connect()?;
            if let Some(server_id) = project.server_id.as_deref() {
                let mut rows = conn
                    .query(
                        "select 1 from servers where server_id = ?1 limit 1",
                        (server_id,),
                    )
                    .await
                    .map_err(turso_error)?;
                if rows.next().await.map_err(turso_error)?.is_none() {
                    return Err(LakeCatError::NotFound {
                        object: "server",
                        name: server_id.to_string(),
                    });
                }
            }
            conn.execute(
                "insert into projects (
                    project_id, display_name, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4)
                 on conflict(project_id) do update set
                    display_name = excluded.display_name,
                    record_json = excluded.record_json,
                    updated_at = excluded.updated_at",
                (
                    project.project_id.as_str(),
                    project.display_name.as_deref(),
                    encode_json(&project)?,
                    project.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            Ok(project)
        }

        async fn list_projects(&self) -> LakeCatResult<Vec<ProjectRecord>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json from projects
                     order by project_id",
                    (),
                )
                .await
                .map_err(turso_error)?;
            let mut projects = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                projects.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(projects)
        }

        async fn upsert_warehouse(
            &self,
            warehouse: WarehouseRecord,
        ) -> LakeCatResult<WarehouseRecord> {
            warehouse.validate()?;
            let conn = self.connect()?;
            let project_exists = {
                let mut rows = conn
                    .query(
                        "select 1 from projects where project_id = ?1 limit 1",
                        (warehouse.project_id.as_str(),),
                    )
                    .await
                    .map_err(turso_error)?;
                rows.next().await.map_err(turso_error)?.is_some()
            };
            if !project_exists {
                return Err(LakeCatError::NotFound {
                    object: "project",
                    name: warehouse.project_id.clone(),
                });
            }
            conn.execute(
                "insert into warehouses (
                    warehouse, project_id, storage_root, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)
                 on conflict(warehouse) do update set
                    project_id = excluded.project_id,
                    storage_root = excluded.storage_root,
                    record_json = excluded.record_json,
                    updated_at = excluded.updated_at",
                (
                    warehouse.warehouse.as_str(),
                    warehouse.project_id.as_str(),
                    warehouse.storage_root.as_deref(),
                    encode_json(&warehouse)?,
                    warehouse.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            Ok(warehouse)
        }

        async fn load_warehouse(
            &self,
            warehouse: &WarehouseName,
        ) -> LakeCatResult<WarehouseRecord> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json from warehouses
                     where warehouse = ?1",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            rows.next()
                .await
                .map_err(turso_error)?
                .map(|row| decode_json(row_string(&row, 0)?))
                .transpose()?
                .ok_or_else(|| LakeCatError::NotFound {
                    object: "warehouse",
                    name: warehouse.as_str().to_string(),
                })
        }

        async fn list_warehouses(&self) -> LakeCatResult<Vec<WarehouseRecord>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json from warehouses
                     order by warehouse",
                    (),
                )
                .await
                .map_err(turso_error)?;
            let mut warehouses = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                warehouses.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(warehouses)
        }

        async fn list_project_warehouses(
            &self,
            project_id: &str,
        ) -> LakeCatResult<Vec<WarehouseRecord>> {
            validate_project_id(project_id)?;
            let conn = self.connect()?;
            let project_exists = {
                let mut rows = conn
                    .query(
                        "select 1 from projects where project_id = ?1 limit 1",
                        (project_id,),
                    )
                    .await
                    .map_err(turso_error)?;
                rows.next().await.map_err(turso_error)?.is_some()
            };
            if !project_exists {
                return Err(LakeCatError::NotFound {
                    object: "project",
                    name: project_id.to_string(),
                });
            }
            let mut rows = conn
                .query(
                    "select record_json from warehouses
                     where project_id = ?1
                     order by warehouse",
                    (project_id,),
                )
                .await
                .map_err(turso_error)?;
            let mut warehouses = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                warehouses.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(warehouses)
        }

        async fn upsert_view(&self, view: ViewRecord) -> LakeCatResult<ViewRecord> {
            view.validate()?;
            let conn = self.connect()?;
            let view_key = view_key(&view);
            conn.execute(
                "insert into views (
                    view_key, warehouse, namespace_path, view_name, dialect, record_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 on conflict(view_key) do update set
                    dialect = excluded.dialect,
                    record_json = excluded.record_json,
                    updated_at = excluded.updated_at",
                (
                    view_key.as_str(),
                    view.warehouse.as_str(),
                    view.namespace.path().as_str(),
                    view.name.as_str(),
                    view.dialect.as_str(),
                    encode_json(&view)?,
                    view.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            Ok(view)
        }

        async fn load_view(
            &self,
            warehouse: &WarehouseName,
            namespace: &Namespace,
            view: &TableName,
        ) -> LakeCatResult<ViewRecord> {
            let conn = self.connect()?;
            let view_key = view_key_parts(warehouse, namespace, view);
            conn.query(
                "select record_json from views
                 where view_key = ?1
                 limit 1",
                (view_key.as_str(),),
            )
            .await
            .map_err(turso_error)?
            .next()
            .await
            .map_err(turso_error)?
            .map(|row| decode_json(row_string(&row, 0)?))
            .transpose()?
            .ok_or_else(|| LakeCatError::NotFound {
                object: "view",
                name: view.as_str().to_string(),
            })
        }

        async fn drop_view(
            &self,
            warehouse: &WarehouseName,
            namespace: &Namespace,
            view: &TableName,
        ) -> LakeCatResult<ViewRecord> {
            let conn = self.connect()?;
            let view_key = view_key_parts(warehouse, namespace, view);
            let record = self.load_view(warehouse, namespace, view).await?;
            conn.execute(
                "delete from views where view_key = ?1",
                (view_key.as_str(),),
            )
            .await
            .map_err(turso_error)?;
            Ok(record)
        }

        async fn list_views(
            &self,
            warehouse: &WarehouseName,
            namespace: &Namespace,
        ) -> LakeCatResult<Vec<ViewRecord>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json from views
                     where warehouse = ?1 and namespace_path = ?2
                     order by view_name",
                    (warehouse.as_str(), namespace.path().as_str()),
                )
                .await
                .map_err(turso_error)?;
            let mut views = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                views.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(views)
        }

        async fn record_audit_event(&self, event: CatalogAuditEvent) -> LakeCatResult<()> {
            let conn = self.connect()?;
            let event_id = content_hash_json(&serde_json::json!({
                "event-type": &event.event_type,
                "table": &event.table,
                "principal": &event.principal,
                "request-hash": &event.request_hash,
                "payload": &event.payload,
                "created-at": event.created_at.to_rfc3339(),
            }))?;
            conn.execute(
                "insert into audit_events (
                    event_id, event_type, table_key, principal_json,
                    request_hash, event_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                (
                    event_id.as_str(),
                    event.event_type.as_str(),
                    event.table.as_ref().map(table_key),
                    encode_json(&event.principal)?,
                    event.request_hash.as_deref(),
                    encode_json(&event.payload)?,
                    event.created_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;

            let outbox_payload = serde_json::json!({
                "audit-event-id": event_id,
                "event-type": event.event_type,
                "table": event.table,
                "payload": event.payload,
            });
            tx_insert_outbox_event(&conn, &outbox_payload, event.created_at).await?;
            Ok(())
        }

        async fn pending_outbox_events(
            &self,
            sink: Option<&str>,
            limit: usize,
        ) -> LakeCatResult<Vec<OutboxEvent>> {
            let conn = self.connect()?;
            let limit = checked_i64(limit as u64, "outbox event limit")?;
            let mut rows = if let Some(sink) = sink {
                conn.query(
                    "select event_id, sink, event_type, payload_json, created_at, delivered_at
                     from outbox_events
                     where delivered_at is null and sink = ?1
                     order by created_at, event_id
                     limit ?2",
                    (sink, limit),
                )
                .await
                .map_err(turso_error)?
            } else {
                conn.query(
                    "select event_id, sink, event_type, payload_json, created_at, delivered_at
                     from outbox_events
                     where delivered_at is null
                     order by created_at, event_id
                     limit ?1",
                    (limit,),
                )
                .await
                .map_err(turso_error)?
            };

            let mut events = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                events.push(outbox_event_from_row(&row)?);
            }
            Ok(events)
        }

        async fn mark_outbox_delivered(&self, event_ids: &[String]) -> LakeCatResult<usize> {
            if event_ids.is_empty() {
                return Ok(0);
            }
            let mut conn = self.connect()?;
            let tx = conn.transaction().await.map_err(turso_error)?;
            let delivered_at = Utc::now().to_rfc3339();
            let mut delivered = 0usize;
            for event_id in event_ids {
                let changed = tx
                    .execute(
                        "update outbox_events
                         set delivered_at = ?2
                         where event_id = ?1 and delivered_at is null",
                        (event_id.as_str(), delivered_at.as_str()),
                    )
                    .await
                    .map_err(turso_error)?;
                delivered += changed as usize;
            }
            tx.commit().await.map_err(turso_error)?;
            Ok(delivered)
        }

        async fn upsert_storage_profile(
            &self,
            profile: StorageProfile,
        ) -> LakeCatResult<StorageProfile> {
            profile.validate()?;
            let conn = self.connect()?;
            conn.execute(
                "insert into storage_profiles (
                    profile_key, profile_id, warehouse, location_prefix,
                    provider, issuance_mode, profile_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 on conflict(profile_key) do update set
                    location_prefix = excluded.location_prefix,
                    provider = excluded.provider,
                    issuance_mode = excluded.issuance_mode,
                    profile_json = excluded.profile_json,
                    updated_at = excluded.updated_at",
                (
                    storage_profile_key(&profile.warehouse, &profile.profile_id),
                    profile.profile_id.as_str(),
                    profile.warehouse.as_str(),
                    profile.location_prefix.as_str(),
                    profile.provider.as_str(),
                    profile.issuance_mode.as_str(),
                    encode_json(&profile)?,
                    Utc::now().to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            Ok(profile)
        }

        async fn list_storage_profiles(
            &self,
            warehouse: &WarehouseName,
        ) -> LakeCatResult<Vec<StorageProfile>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select profile_json from storage_profiles
                     where warehouse = ?1
                     order by profile_id",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut profiles = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                profiles.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(profiles)
        }

        async fn storage_profile_for_table(
            &self,
            table: &TableRecord,
        ) -> LakeCatResult<StorageProfile> {
            let profiles = self.list_storage_profiles(&table.ident.warehouse).await?;
            Ok(storage_profile_match(profiles.iter(), table)
                .unwrap_or_else(|| StorageProfile::inferred_for_table(table)))
        }

        async fn upsert_policy_binding(
            &self,
            binding: PolicyBinding,
        ) -> LakeCatResult<PolicyBinding> {
            let conn = self.connect()?;
            conn.execute(
                "insert into policy_bindings (
                    policy_key, policy_id, warehouse, namespace_path, table_name,
                    enforced, binding_json, updated_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 on conflict(policy_key) do update set
                    namespace_path = excluded.namespace_path,
                    table_name = excluded.table_name,
                    enforced = excluded.enforced,
                    binding_json = excluded.binding_json,
                    updated_at = excluded.updated_at",
                (
                    policy_binding_key(&binding.warehouse, &binding.policy_id),
                    binding.policy_id.as_str(),
                    binding.warehouse.as_str(),
                    binding.namespace.as_ref().map(Namespace::path),
                    binding.table.as_ref().map(TableName::as_str),
                    if binding.enforced { 1_i64 } else { 0_i64 },
                    encode_json(&binding)?,
                    binding.updated_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            Ok(binding)
        }

        async fn list_policy_bindings(
            &self,
            warehouse: &WarehouseName,
        ) -> LakeCatResult<Vec<PolicyBinding>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select binding_json from policy_bindings
                     where warehouse = ?1
                     order by policy_id",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut bindings = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                bindings.push(decode_json(row_string(&row, 0)?)?);
            }
            Ok(bindings)
        }

        async fn policy_bindings_for_table(
            &self,
            table: &TableIdent,
        ) -> LakeCatResult<Vec<PolicyBinding>> {
            let bindings = self.list_policy_bindings(&table.warehouse).await?;
            Ok(policy_bindings_for_table(bindings.iter(), table))
        }
    }

    const TURSO_MIGRATION: &[&str] = &[
        "create table if not exists servers (
            server_id text primary key,
            display_name text,
            endpoint_url text,
            record_json text not null,
            updated_at text not null
        )",
        "create table if not exists projects (
            project_id text primary key,
            display_name text,
            record_json text not null,
            updated_at text not null
        )",
        "create table if not exists warehouses (
            warehouse text primary key,
            project_id text not null,
            storage_root text,
            record_json text not null,
            updated_at text not null
        )",
        "create table if not exists namespaces (
            warehouse text not null,
            namespace_path text not null,
            namespace_json text not null,
            primary key (warehouse, namespace_path)
        )",
        "create table if not exists tables (
            table_key text primary key,
            warehouse text not null,
            namespace_path text not null,
            table_name text not null,
            metadata_location text,
            version integer not null,
            record_json text not null,
            updated_at text not null
        )",
        "create index if not exists idx_tables_warehouse_namespace
            on tables (warehouse, namespace_path, table_name)",
        "create table if not exists metadata_pointer_log (
            table_key text not null,
            sequence_number integer not null,
            previous_metadata_location text,
            new_metadata_location text,
            principal_json text not null,
            request_hash text not null,
            committed_at text not null,
            record_json text not null,
            primary key (table_key, sequence_number)
        )",
        "create table if not exists idempotency_records (
            idem_key text primary key,
            table_key text not null,
            request_hash text not null,
            response_json text not null,
            created_at text not null
        )",
        "create table if not exists audit_events (
            event_id text primary key,
            event_type text not null,
            table_key text,
            principal_json text not null,
            request_hash text,
            event_json text not null,
            created_at text not null
        )",
        "create table if not exists outbox_events (
            event_id text primary key,
            sink text not null,
            event_type text not null,
            payload_json text not null,
            created_at text not null,
            delivered_at text
        )",
        "create table if not exists storage_profiles (
            profile_key text primary key,
            profile_id text not null,
            warehouse text not null,
            location_prefix text not null,
            provider text not null,
            issuance_mode text not null,
            profile_json text not null,
            updated_at text not null
        )",
        "create index if not exists idx_storage_profiles_warehouse
            on storage_profiles (warehouse, profile_id)",
        "create table if not exists views (
            view_key text primary key,
            warehouse text not null,
            namespace_path text not null,
            view_name text not null,
            dialect text not null,
            record_json text not null,
            updated_at text not null
        )",
        "create index if not exists idx_views_warehouse_namespace
            on views (warehouse, namespace_path, view_name)",
        "create table if not exists policy_bindings (
            policy_key text primary key,
            policy_id text not null,
            warehouse text not null,
            namespace_path text,
            table_name text,
            enforced integer not null,
            binding_json text not null,
            updated_at text not null
        )",
        "create index if not exists idx_policy_bindings_warehouse
            on policy_bindings (warehouse, policy_id)",
        "create table if not exists soft_deletes (
            table_key text primary key,
            warehouse text not null,
            namespace_path text not null,
            table_name text not null,
            metadata_location text,
            version integer not null,
            principal_json text not null,
            authorization_receipt_json text,
            record_json text not null,
            deleted_at text not null
        )",
        "create index if not exists idx_soft_deletes_warehouse
            on soft_deletes (warehouse, namespace_path, table_name)",
    ];

    fn encode_json(value: impl serde::Serialize) -> LakeCatResult<String> {
        serde_json::to_string(&value)
            .map_err(|err| LakeCatError::Internal(format!("failed to encode store JSON: {err}")))
    }

    fn decode_json<T: DeserializeOwned>(value: String) -> LakeCatResult<T> {
        serde_json::from_str(&value)
            .map_err(|err| LakeCatError::Internal(format!("failed to decode store JSON: {err}")))
    }

    fn decode_namespace(value: String) -> LakeCatResult<Namespace> {
        Namespace::new(decode_json::<Vec<String>>(value)?)
    }

    fn idempotency_record_key(ident: &TableIdent, idempotency_key: &str) -> String {
        format!("{}:{idempotency_key}", ident.stable_id())
    }

    fn checked_i64(value: u64, name: &str) -> LakeCatResult<i64> {
        i64::try_from(value)
            .map_err(|_| LakeCatError::InvalidArgument(format!("{name} exceeds i64 range")))
    }

    fn row_string(row: &Row, idx: usize) -> LakeCatResult<String> {
        match row.get_value(idx).map_err(turso_error)? {
            TursoValue::Text(value) => Ok(value),
            value => Err(LakeCatError::Internal(format!(
                "Turso catalog store expected text at column {idx}, got {value:?}"
            ))),
        }
    }

    fn row_optional_string(row: &Row, idx: usize) -> LakeCatResult<Option<String>> {
        match row.get_value(idx).map_err(turso_error)? {
            TursoValue::Null => Ok(None),
            TursoValue::Text(value) => Ok(Some(value)),
            value => Err(LakeCatError::Internal(format!(
                "Turso catalog store expected nullable text at column {idx}, got {value:?}"
            ))),
        }
    }

    #[cfg(test)]
    fn row_i64(row: &Row, idx: usize) -> LakeCatResult<i64> {
        match row.get_value(idx).map_err(turso_error)? {
            TursoValue::Integer(value) => Ok(value),
            value => Err(LakeCatError::Internal(format!(
                "Turso catalog store expected integer at column {idx}, got {value:?}"
            ))),
        }
    }

    fn outbox_event_from_row(row: &Row) -> LakeCatResult<OutboxEvent> {
        Ok(OutboxEvent {
            event_id: row_string(row, 0)?,
            sink: row_string(row, 1)?,
            event_type: row_string(row, 2)?,
            payload: decode_json::<JsonValue>(row_string(row, 3)?)?,
            created_at: parse_turso_datetime(row_string(row, 4)?, "outbox created_at")?,
            delivered_at: row_optional_string(row, 5)?
                .map(|value| parse_turso_datetime(value, "outbox delivered_at"))
                .transpose()?,
        })
    }

    fn parse_turso_datetime(value: String, name: &str) -> LakeCatResult<chrono::DateTime<Utc>> {
        chrono::DateTime::parse_from_rfc3339(&value)
            .map(|datetime| datetime.with_timezone(&Utc))
            .map_err(|err| {
                LakeCatError::Internal(format!("failed to parse {name} timestamp: {err}"))
            })
    }

    async fn tx_insert_outbox_event(
        conn: &Connection,
        payload: &JsonValue,
        created_at: chrono::DateTime<Utc>,
    ) -> LakeCatResult<()> {
        conn.execute(
            "insert into outbox_events (
                event_id, sink, event_type, payload_json, created_at
             )
             values (?1, ?2, ?3, ?4, ?5)",
            (
                content_hash_json(payload)?,
                "lakecat.lineage-and-graph",
                payload["event-type"].as_str(),
                encode_json(payload)?,
                created_at.to_rfc3339(),
            ),
        )
        .await
        .map_err(turso_error)?;
        Ok(())
    }

    fn is_unique_violation(err: &turso::Error) -> bool {
        matches!(err, turso::Error::Constraint(message) if message.contains("UNIQUE") || message.contains("PRIMARY KEY"))
    }

    fn turso_error(err: turso::Error) -> LakeCatError {
        LakeCatError::Internal(format!("Turso catalog store error: {err}"))
    }

    #[cfg(test)]
    mod tests {
        use std::collections::BTreeMap;

        use lakecat_core::{Principal, TableName};

        use crate::{
            CredentialIssuanceMode, PolicyBinding, ServerRecord, StorageProvider, ViewRecord,
        };

        use super::*;

        #[tokio::test]
        async fn turso_store_persists_server_records() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            assert_eq!(store.list_servers().await.unwrap(), vec![]);

            let record = ServerRecord::new(
                "lakecat-local",
                Some("Local LakeCat".to_string()),
                Some("http://127.0.0.1:8181".to_string()),
                BTreeMap::from([("deployment".to_string(), "local".to_string())]),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_server(record).await.unwrap();

            let updated = ServerRecord::new(
                "lakecat-local",
                Some("Local QueryGraph LakeCat".to_string()),
                Some("http://127.0.0.1:8182".to_string()),
                BTreeMap::from([("deployment".to_string(), "dev".to_string())]),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_server(updated.clone()).await.unwrap();

            assert_eq!(store.list_servers().await.unwrap(), vec![updated]);
        }

        #[tokio::test]
        async fn turso_store_persists_warehouse_records() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            assert_eq!(store.list_warehouses().await.unwrap(), vec![]);
            let project = ProjectRecord::new(
                "default",
                None,
                Some("Default Project".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_project(project).await.unwrap();

            let warehouse = WarehouseName::new("local").unwrap();
            let record = WarehouseRecord::new(
                warehouse.clone(),
                "default",
                Some("file:///tmp/lakecat".to_string()),
                BTreeMap::from([("region".to_string(), "local".to_string())]),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_warehouse(record).await.unwrap();

            let updated = WarehouseRecord::new(
                warehouse.clone(),
                "default",
                Some("file:///tmp/lakecat-updated".to_string()),
                BTreeMap::from([("region".to_string(), "test".to_string())]),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_warehouse(updated.clone()).await.unwrap();

            assert_eq!(store.load_warehouse(&warehouse).await.unwrap(), updated);
            assert!(matches!(
                store
                    .load_warehouse(&WarehouseName::new("missing").unwrap())
                    .await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "warehouse" && name == "missing"
            ));
            assert_eq!(
                store.list_warehouses().await.unwrap(),
                vec![updated.clone()]
            );
            assert_eq!(
                store.list_project_warehouses("default").await.unwrap(),
                vec![updated.clone()]
            );
            assert!(matches!(
                store.list_project_warehouses("missing-project").await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "project" && name == "missing-project"
            ));

            let missing_project = WarehouseRecord::new(
                WarehouseName::new("orphaned").unwrap(),
                "missing-project",
                Some("file:///tmp/orphaned".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            assert!(matches!(
                store.upsert_warehouse(missing_project).await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "project" && name == "missing-project"
            ));
        }

        #[tokio::test]
        async fn turso_store_persists_project_records() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            assert_eq!(store.list_projects().await.unwrap(), vec![]);

            let record = ProjectRecord::new(
                "default",
                Some("lakecat-local".to_string()),
                Some("Default Project".to_string()),
                BTreeMap::from([("owner".to_string(), "querygraph".to_string())]),
                Principal::anonymous(),
            )
            .unwrap();
            store
                .upsert_server(
                    ServerRecord::new(
                        "lakecat-local",
                        Some("Local LakeCat".to_string()),
                        None,
                        BTreeMap::new(),
                        Principal::anonymous(),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
            store.upsert_project(record).await.unwrap();

            let updated = ProjectRecord::new(
                "default",
                Some("lakecat-local".to_string()),
                Some("QueryGraph Project".to_string()),
                BTreeMap::from([("owner".to_string(), "lakecat".to_string())]),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_project(updated.clone()).await.unwrap();

            assert_eq!(store.list_projects().await.unwrap(), vec![updated]);

            let missing_server = ProjectRecord::new(
                "orphaned",
                Some("missing-server".to_string()),
                Some("Orphaned Project".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            assert!(matches!(
                store.upsert_project(missing_server).await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "server" && name == "missing-server"
            ));
        }

        #[tokio::test]
        async fn turso_store_persists_view_records() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            assert_eq!(
                store.list_views(&warehouse, &namespace).await.unwrap(),
                vec![]
            );

            let view = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("active_customers").unwrap(),
                "select * from customers where active",
                "sql",
                Some(1),
                BTreeMap::from([("owner".to_string(), "querygraph".to_string())]),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_view(view).await.unwrap();

            let updated = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("active_customers").unwrap(),
                "select id, email from customers where active",
                "sql",
                Some(2),
                BTreeMap::from([("owner".to_string(), "lakecat".to_string())]),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_view(updated.clone()).await.unwrap();

            assert_eq!(
                store
                    .load_view(
                        &warehouse,
                        &namespace,
                        &TableName::new("active_customers").unwrap()
                    )
                    .await
                    .unwrap(),
                updated.clone()
            );
            assert!(matches!(
                store
                    .load_view(&warehouse, &namespace, &TableName::new("missing_view").unwrap())
                    .await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "view" && name == "missing_view"
            ));
            assert_eq!(
                store.list_views(&warehouse, &namespace).await.unwrap(),
                vec![updated.clone()]
            );
            assert_eq!(
                store
                    .drop_view(
                        &warehouse,
                        &namespace,
                        &TableName::new("active_customers").unwrap()
                    )
                    .await
                    .unwrap(),
                updated
            );
            assert_eq!(
                store.list_views(&warehouse, &namespace).await.unwrap(),
                Vec::<ViewRecord>::new()
            );
            assert!(matches!(
                store
                    .drop_view(
                        &warehouse,
                        &namespace,
                        &TableName::new("active_customers").unwrap()
                    )
                    .await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "view" && name == "active_customers"
            ));
        }

        #[tokio::test]
        async fn turso_store_round_trips_namespaces_tables_and_idempotent_commits() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);

            let namespace = "default".parse::<Namespace>().unwrap();
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            assert_eq!(
                store.list_namespaces(&warehouse).await.unwrap(),
                vec![namespace.clone()]
            );

            let ident = TableIdent::new(
                warehouse.clone(),
                namespace,
                TableName::new("events").unwrap(),
            );
            let table = TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            store.create_table(table).await.unwrap();
            assert_eq!(store.load_table(&ident).await.unwrap().version, 0);

            let commit = TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: None,
                idempotency_key: Some("commit-1".to_string()),
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: Some(serde_json::json!({
                    "engine": "typesec",
                    "allowed": true,
                    "action": "table.commit"
                })),
            };
            let committed = store.commit_table(&ident, commit.clone()).await.unwrap();
            assert_eq!(committed.version, 1);
            assert_eq!(
                committed.metadata_location.as_deref(),
                Some("file:///tmp/events/metadata/00001.json")
            );
            let replayed = store.commit_table(&ident, commit).await.unwrap();
            assert_eq!(replayed.version, 1);

            let mismatched = TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00002.json".to_string()),
                new_metadata: None,
                idempotency_key: Some("commit-1".to_string()),
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: Some(serde_json::json!({
                    "engine": "typesec",
                    "allowed": true,
                    "action": "table.commit"
                })),
            };
            let err = store.commit_table(&ident, mismatched).await.unwrap_err();
            assert!(
                err.to_string()
                    .contains("idempotency key reused with different commit request")
            );

            let commit_count = store.count_rows("metadata_pointer_log").await.unwrap();
            assert_eq!(commit_count, 1);
            let commit_records = store.table_commit_records(&ident, 1, None).await.unwrap();
            assert_eq!(commit_records.len(), 1);
            assert_eq!(commit_records[0].sequence_number, 1);
            assert_eq!(
                commit_records[0].idempotency_key_sha256.as_deref(),
                Some(content_hash_bytes("commit-1".as_bytes()).as_str())
            );
            assert_eq!(
                commit_records[0].new_metadata_location.as_deref(),
                Some("file:///tmp/events/metadata/00001.json")
            );
            assert_eq!(
                store.table_commit_records(&ident, 2, None).await.unwrap(),
                vec![]
            );
            let audit_count = store.count_rows("audit_events").await.unwrap();
            assert_eq!(audit_count, 1);
            let outbox_count = store.count_rows("outbox_events").await.unwrap();
            assert_eq!(outbox_count, 1);

            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].event_type, "table.commit");
            assert_eq!(
                pending[0].payload["commit"]["new_metadata_location"],
                serde_json::json!("file:///tmp/events/metadata/00001.json")
            );
            assert_eq!(
                pending[0].payload["commit"]["idempotency_key_sha256"],
                serde_json::json!(content_hash_bytes("commit-1".as_bytes()))
            );
            assert!(!pending[0].payload.to_string().contains("commit-1"));
            assert_eq!(
                pending[0].payload["authorization-receipt"]["engine"],
                serde_json::json!("typesec")
            );
            let event_ids = pending
                .iter()
                .map(|event| event.event_id.clone())
                .collect::<Vec<_>>();
            assert_eq!(store.mark_outbox_delivered(&event_ids).await.unwrap(), 1);
            assert!(
                store
                    .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(store.mark_outbox_delivered(&event_ids).await.unwrap(), 0);
        }

        #[tokio::test]
        async fn turso_store_rejects_stale_metadata_pointer_commits() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(warehouse, namespace, TableName::new("events").unwrap());
            let table = TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            store.create_table(table).await.unwrap();

            let err = store
                .commit_table(
                    &ident,
                    TableCommit {
                        requirements: vec![],
                        updates: vec![serde_json::json!({"action": "noop"})],
                        expected_previous_metadata_location: Some(
                            "file:///tmp/events/metadata/stale.json".to_string(),
                        ),
                        new_metadata_location: Some(
                            "file:///tmp/events/metadata/00001.json".to_string(),
                        ),
                        new_metadata: None,
                        idempotency_key: None,
                        idempotency_request_hash: None,
                        principal: Principal::anonymous(),
                        authorization_receipt: None,
                    },
                )
                .await
                .expect_err("stale metadata pointer must conflict");

            assert!(matches!(err, LakeCatError::Conflict(_)));
            assert_eq!(store.load_table(&ident).await.unwrap().version, 0);
            assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
        }

        #[tokio::test]
        async fn turso_store_records_governed_scan_audit_outbox_events() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            store
                .record_audit_event(
                    CatalogAuditEvent::new(
                        "table.scan-planned",
                        Some(ident.clone()),
                        Principal::anonymous(),
                        serde_json::json!({
                            "event-type": "table.scan-planned",
                            "table": ident,
                            "authorization-receipt": {
                                "engine": "typesec",
                                "allowed": true,
                                "action": "table-plan-scan"
                            },
                            "planned-by": "lakecat-sail",
                            "scan-task-count": 2
                        }),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(store.count_rows("audit_events").await.unwrap(), 1);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].event_type, "table.scan-planned");
            assert_eq!(
                pending[0].payload["payload"]["authorization-receipt"]["engine"],
                serde_json::json!("typesec")
            );
            assert_eq!(
                pending[0].payload["payload"]["scan-task-count"],
                serde_json::json!(2)
            );

            store
                .record_audit_event(
                    CatalogAuditEvent::new(
                        "table.scan-tasks-fetched",
                        Some(ident.clone()),
                        Principal::anonymous(),
                        serde_json::json!({
                            "event-type": "table.scan-tasks-fetched",
                            "table": ident,
                            "authorization-receipt": {
                                "engine": "typesec",
                                "allowed": true,
                                "action": "table-plan-scan"
                            },
                            "planned-by": "lakecat-sail",
                            "plan-task": "lakecat:plan:abc",
                            "file-scan-task-count": 3,
                            "delete-file-count": 1
                        }),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(store.count_rows("audit_events").await.unwrap(), 2);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 2);
            let fetched = pending
                .iter()
                .find(|event| event.event_type == "table.scan-tasks-fetched")
                .expect("scan task fetch event");
            assert_eq!(
                fetched.payload["payload"]["file-scan-task-count"],
                serde_json::json!(3)
            );
            assert_eq!(
                fetched.payload["payload"]["authorization-receipt"]["engine"],
                serde_json::json!("typesec")
            );

            store
                .record_audit_event(
                    CatalogAuditEvent::new(
                        "table.loaded",
                        Some(ident.clone()),
                        Principal::anonymous(),
                        serde_json::json!({
                            "event-type": "table.loaded",
                            "table": ident,
                            "authorization-receipt": {
                                "engine": "typesec",
                                "allowed": true,
                                "action": "table-load"
                            },
                            "metadata-location": "file:///tmp/events/metadata/00000.json",
                            "version": 7
                        }),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(store.count_rows("audit_events").await.unwrap(), 3);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 3);
            let loaded = pending
                .iter()
                .find(|event| event.event_type == "table.loaded")
                .expect("table loaded event");
            assert_eq!(
                loaded.payload["payload"]["metadata-location"],
                serde_json::json!("file:///tmp/events/metadata/00000.json")
            );
            assert_eq!(
                loaded.payload["payload"]["authorization-receipt"]["action"],
                serde_json::json!("table-load")
            );

            store
                .record_audit_event(
                    CatalogAuditEvent::new(
                        "table.created",
                        Some(ident.clone()),
                        Principal::anonymous(),
                        serde_json::json!({
                            "event-type": "table.created",
                            "table": ident,
                            "authorization-receipt": {
                                "engine": "typesec",
                                "allowed": true,
                                "action": "table-create"
                            },
                            "metadata-location": "file:///tmp/events/metadata/00000.json",
                            "location": "file:///tmp/events",
                            "version": 0
                        }),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(store.count_rows("audit_events").await.unwrap(), 4);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 4);
            let created = pending
                .iter()
                .find(|event| event.event_type == "table.created")
                .expect("table created event");
            assert_eq!(
                created.payload["payload"]["location"],
                serde_json::json!("file:///tmp/events")
            );
            assert_eq!(
                created.payload["payload"]["authorization-receipt"]["action"],
                serde_json::json!("table-create")
            );

            store
                .record_audit_event(
                    CatalogAuditEvent::new(
                        "querygraph.bootstrap",
                        None,
                        Principal::anonymous(),
                        serde_json::json!({
                            "event-type": "querygraph.bootstrap",
                            "authorization-receipt": {
                                "engine": "typesec",
                                "allowed": true,
                                "action": "graph-read"
                            },
                            "warehouse": "local",
                            "table-count": 1
                        }),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(store.count_rows("audit_events").await.unwrap(), 5);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 5);
            let bootstrap = pending
                .iter()
                .find(|event| event.event_type == "querygraph.bootstrap")
                .expect("querygraph bootstrap event");
            assert_eq!(
                bootstrap.payload["payload"]["authorization-receipt"]["action"],
                serde_json::json!("graph-read")
            );
            assert_eq!(
                bootstrap.payload["payload"]["table-count"],
                serde_json::json!(1)
            );

            for (event_type, payload) in [
                (
                    "catalog.config-read",
                    serde_json::json!({
                        "event-type": "catalog.config-read",
                        "authorization-receipt": {
                            "engine": "typesec",
                            "allowed": true,
                            "action": "catalog-config"
                        },
                        "warehouse": "local"
                    }),
                ),
                (
                    "namespace.created",
                    serde_json::json!({
                        "event-type": "namespace.created",
                        "authorization-receipt": {
                            "engine": "typesec",
                            "allowed": true,
                            "action": "namespace-create"
                        },
                        "warehouse": "local",
                        "namespace": ["default"]
                    }),
                ),
                (
                    "namespace.listed",
                    serde_json::json!({
                        "event-type": "namespace.listed",
                        "authorization-receipt": {
                            "engine": "typesec",
                            "allowed": true,
                            "action": "namespace-list"
                        },
                        "warehouse": "local",
                        "namespace-count": 1
                    }),
                ),
            ] {
                store
                    .record_audit_event(
                        CatalogAuditEvent::new(event_type, None, Principal::anonymous(), payload)
                            .unwrap(),
                    )
                    .await
                    .unwrap();
            }

            assert_eq!(store.count_rows("audit_events").await.unwrap(), 8);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 8);
            let namespace_listed = pending
                .iter()
                .find(|event| event.event_type == "namespace.listed")
                .expect("namespace listed event");
            assert_eq!(
                namespace_listed.payload["payload"]["namespace-count"],
                serde_json::json!(1)
            );

            store
                .record_audit_event(
                    CatalogAuditEvent::new(
                        "credentials.vend-attempted",
                        Some(ident.clone()),
                        Principal::anonymous(),
                        serde_json::json!({
                            "event-type": "credentials.vend-attempted",
                            "table": ident,
                            "authorization-receipt": {
                                "engine": "typesec",
                                "allowed": true,
                                "action": "credentials-vend"
                            },
                            "storage-location": "file:///tmp/events",
                            "credential-count": 0,
                            "mode": "governed-read-required"
                        }),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(store.count_rows("audit_events").await.unwrap(), 9);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 9);
            let credentials = pending
                .iter()
                .find(|event| event.event_type == "credentials.vend-attempted")
                .expect("credentials vend attempted event");
            assert_eq!(
                credentials.payload["payload"]["credential-count"],
                serde_json::json!(0)
            );
            assert_eq!(
                credentials.payload["payload"]["authorization-receipt"]["action"],
                serde_json::json!("credentials-vend")
            );
        }

        #[tokio::test]
        async fn turso_store_allows_only_one_concurrent_metadata_pointer_commit() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(warehouse, namespace, TableName::new("events").unwrap());
            let table = TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            store.create_table(table).await.unwrap();

            let commit_a = TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "append", "writer": "a"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001-a.json".to_string()),
                new_metadata: None,
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            };
            let commit_b = TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "append", "writer": "b"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001-b.json".to_string()),
                new_metadata: None,
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            };

            let (result_a, result_b) = tokio::join!(
                store.commit_table(&ident, commit_a),
                store.commit_table(&ident, commit_b)
            );
            let results = [result_a, result_b];
            let success_count = results.iter().filter(|result| result.is_ok()).count();
            let conflict_count = results
                .iter()
                .filter(|result| matches!(result, Err(LakeCatError::Conflict(_))))
                .count();

            assert_eq!(success_count, 1);
            assert_eq!(conflict_count, 1);
            assert_eq!(store.load_table(&ident).await.unwrap().version, 1);
            assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 1);
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 1);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 1);
        }

        #[tokio::test]
        async fn turso_store_persists_storage_profiles_and_matches_longest_prefix() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace,
                TableName::new("events").unwrap(),
            );
            let table = TableRecord::new(
                ident,
                "file:///tmp/events/tenant-a/table".to_string(),
                Some("file:///tmp/events/tenant-a/table/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );

            let broad = StorageProfile::new(
                "local-broad",
                warehouse.clone(),
                "file:///tmp/events",
                StorageProvider::File,
                CredentialIssuanceMode::LocalFileNoSecret,
                None,
                BTreeMap::new(),
            )
            .unwrap();
            let narrow = StorageProfile::new(
                "local-tenant-a",
                warehouse.clone(),
                "file:///tmp/events/tenant-a",
                StorageProvider::File,
                CredentialIssuanceMode::LocalFileNoSecret,
                None,
                BTreeMap::from([("lakecat.endpoint".to_string(), "local".to_string())]),
            )
            .unwrap();

            store.upsert_storage_profile(broad).await.unwrap();
            store.upsert_storage_profile(narrow.clone()).await.unwrap();

            let profiles = store.list_storage_profiles(&warehouse).await.unwrap();
            assert_eq!(profiles.len(), 2);
            let matched = store.storage_profile_for_table(&table).await.unwrap();
            assert_eq!(matched.profile_id, narrow.profile_id);
            assert_eq!(
                matched.public_config["lakecat.endpoint"],
                narrow.public_config["lakecat.endpoint"]
            );
        }

        #[tokio::test]
        async fn turso_store_persists_secret_ref_profiles_without_secret_material() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let profile = StorageProfile::new(
                "s3-events",
                warehouse.clone(),
                "s3://lakecat-demo/events",
                StorageProvider::S3,
                CredentialIssuanceMode::ShortLivedSecretRef,
                Some("typesec://lakecat/local/s3-events".to_string()),
                BTreeMap::from([("lakecat.region".to_string(), "us-west-2".to_string())]),
            )
            .unwrap();

            store.upsert_storage_profile(profile).await.unwrap();
            let profiles = store.list_storage_profiles(&warehouse).await.unwrap();
            assert_eq!(profiles.len(), 1);
            assert_eq!(
                profiles[0].secret_ref.as_deref(),
                Some("typesec://lakecat/local/s3-events")
            );

            let embedded_secret = StorageProfile::new(
                "bad-s3-events",
                warehouse,
                "s3://lakecat-demo/events",
                StorageProvider::S3,
                CredentialIssuanceMode::ShortLivedSecretRef,
                Some("typesec://lakecat/local/s3-events?token=secret".to_string()),
                BTreeMap::new(),
            );
            assert!(embedded_secret.is_err());
        }

        #[tokio::test]
        async fn turso_store_persists_policy_bindings_and_matches_table_scope() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let table = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            let binding = PolicyBinding::new(
                "agent-read",
                warehouse.clone(),
                Some(namespace),
                Some(TableName::new("events").unwrap()),
                true,
                serde_json::json!({
                    "uid": "policy:agent-read",
                    "permission": [{"action": "read"}]
                }),
            )
            .unwrap();
            let inactive = PolicyBinding::new(
                "inactive",
                warehouse.clone(),
                None,
                None,
                false,
                serde_json::json!({"uid": "policy:inactive"}),
            )
            .unwrap();

            store.upsert_policy_binding(binding.clone()).await.unwrap();
            store.upsert_policy_binding(inactive).await.unwrap();

            let policies = store.list_policy_bindings(&warehouse).await.unwrap();
            assert_eq!(policies.len(), 2);
            let active = store.policy_bindings_for_table(&table).await.unwrap();
            assert_eq!(active.len(), 1);
            assert_eq!(active[0].policy_id, binding.policy_id);
            assert_eq!(
                active[0].odrl["uid"],
                serde_json::json!("policy:agent-read")
            );
        }

        #[tokio::test]
        async fn turso_store_soft_deletes_tables_from_normal_catalog_reads() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace,
                TableName::new("events").unwrap(),
            );
            let table = TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            store.create_table(table).await.unwrap();
            assert_eq!(store.list_tables(&warehouse).await.unwrap().len(), 1);

            let deleted = store
                .soft_delete_table(
                    &ident,
                    Principal::anonymous(),
                    Some(serde_json::json!({
                        "engine": "typesec",
                        "allowed": true,
                        "action": "table-drop"
                    })),
                )
                .await
                .unwrap();
            assert_eq!(deleted.ident, ident);
            assert!(matches!(
                store.load_table(&ident).await,
                Err(LakeCatError::NotFound { .. })
            ));
            assert_eq!(store.list_tables(&warehouse).await.unwrap(), vec![]);
            assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 1);
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 1);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].event_type, "table.deleted");
        }

        #[tokio::test]
        async fn turso_store_restores_soft_deleted_tables() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace,
                TableName::new("events").unwrap(),
            );
            let table = TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            store.create_table(table).await.unwrap();
            store
                .soft_delete_table(
                    &ident,
                    Principal::anonymous(),
                    Some(serde_json::json!({
                        "engine": "typesec",
                        "allowed": true,
                        "action": "table-drop"
                    })),
                )
                .await
                .unwrap();
            assert!(matches!(
                store.load_table(&ident).await,
                Err(LakeCatError::NotFound { .. })
            ));

            let restored = store
                .restore_table(
                    &ident,
                    Principal::anonymous(),
                    Some(serde_json::json!({
                        "engine": "typesec",
                        "allowed": true,
                        "action": "table-restore"
                    })),
                )
                .await
                .unwrap();
            assert_eq!(restored.ident, ident);
            assert_eq!(store.load_table(&ident).await.unwrap().ident, ident);
            assert_eq!(store.list_tables(&warehouse).await.unwrap().len(), 1);
            assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 0);
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 2);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 2);
            assert!(
                pending
                    .iter()
                    .any(|event| event.event_type == "table.restored")
            );
        }
    }
}
