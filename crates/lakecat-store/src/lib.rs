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
use url::Url;

#[async_trait]
pub trait CatalogStore: Send + Sync + 'static {
    async fn create_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: Namespace,
    ) -> LakeCatResult<()>;
    async fn list_namespaces(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<Namespace>>;
    async fn load_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        self.list_namespaces(warehouse)
            .await?
            .into_iter()
            .find(|candidate| candidate == namespace)
            .ok_or_else(|| namespace_not_found(namespace))
    }
    async fn drop_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        let namespace = self.load_namespace(warehouse, namespace).await?;
        if self
            .list_tables(warehouse)
            .await?
            .iter()
            .any(|table| table.ident.namespace == namespace)
        {
            return Err(namespace_not_empty(&namespace, "tables"));
        }
        if !self.list_views(warehouse, &namespace).await?.is_empty() {
            return Err(namespace_not_empty(&namespace, "views"));
        }
        if self
            .list_policy_bindings(warehouse)
            .await?
            .iter()
            .any(|binding| binding.namespace.as_ref() == Some(&namespace))
        {
            return Err(namespace_not_empty(&namespace, "policy bindings"));
        }
        Ok(namespace)
    }
    async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>>;
    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord>;
    async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord>;
    async fn commit_table(
        &self,
        ident: &TableIdent,
        commit: TableCommit,
    ) -> LakeCatResult<TableRecord>;
    async fn replay_table_commit(
        &self,
        _ident: &TableIdent,
        _idempotency_key: &str,
        _idempotency_request_hash: &str,
    ) -> LakeCatResult<Option<TableRecord>> {
        Ok(None)
    }
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
    async fn upsert_view_if_version(
        &self,
        view: ViewRecord,
        expected_view_version: Option<u64>,
    ) -> LakeCatResult<ViewRecord> {
        if let Some(expected) = expected_view_version {
            validate_expected_view_version(expected)?;
            let current = self
                .load_view(&view.warehouse, &view.namespace, &view.name)
                .await?;
            require_expected_view_version(Some(&current), expected)?;
        }
        self.upsert_view(view).await
    }
    async fn list_view_version_receipts(
        &self,
        _warehouse: &WarehouseName,
        _namespace: &Namespace,
        _view: &TableName,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        Ok(Vec::new())
    }
    async fn list_namespace_view_version_receipts(
        &self,
        _warehouse: &WarehouseName,
        _namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        Ok(Vec::new())
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
        _principal: Principal,
    ) -> LakeCatResult<ViewRecord> {
        self.drop_view_if_version(warehouse, namespace, view, _principal, None)
            .await
    }
    async fn drop_view_if_version(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
        _principal: Principal,
        expected_view_version: Option<u64>,
    ) -> LakeCatResult<ViewRecord> {
        if let Some(expected) = expected_view_version {
            validate_expected_view_version(expected)?;
            let current = self.load_view(warehouse, namespace, view).await?;
            require_expected_view_version(Some(&current), expected)?;
        }
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
        binding.validate()?;
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

    pub fn validate(&self) -> LakeCatResult<()> {
        if self.location.trim().is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "table location must not be empty".to_string(),
            ));
        }
        if self
            .metadata_location
            .as_deref()
            .is_some_and(|location| location.trim().is_empty())
        {
            return Err(LakeCatError::InvalidArgument(
                "table metadata location must not be empty when present".to_string(),
            ));
        }
        if !self.metadata.is_object() {
            return Err(LakeCatError::InvalidArgument(
                "table metadata must be a JSON object".to_string(),
            ));
        }
        validate_table_metadata_format_version(&self.metadata, "table metadata")?;
        Ok(())
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

impl TableCommit {
    pub fn validate(&self) -> LakeCatResult<()> {
        if let Some(idempotency_key) = self.idempotency_key.as_deref() {
            validate_idempotency_key_shape(idempotency_key)?;
        } else if self.idempotency_request_hash.is_some() {
            return Err(LakeCatError::InvalidArgument(
                "table commit idempotency request hash requires an idempotency key".to_string(),
            ));
        }
        if let Some(idempotency_request_hash) = self.idempotency_request_hash.as_deref() {
            validate_idempotency_request_hash_shape(idempotency_request_hash)?;
        }
        if self
            .expected_previous_metadata_location
            .as_deref()
            .is_some_and(|location| location.trim().is_empty())
        {
            return Err(LakeCatError::InvalidArgument(
                "expected table metadata location must not be empty when present".to_string(),
            ));
        }
        if self
            .new_metadata_location
            .as_deref()
            .is_some_and(|location| location.trim().is_empty())
        {
            return Err(LakeCatError::InvalidArgument(
                "new table metadata location must not be empty when present".to_string(),
            ));
        }
        if self
            .new_metadata
            .as_ref()
            .is_some_and(|metadata| !metadata.is_object())
        {
            return Err(LakeCatError::InvalidArgument(
                "new table metadata must be a JSON object".to_string(),
            ));
        }
        if let Some(metadata) = self.new_metadata.as_ref() {
            validate_table_metadata_format_version(metadata, "new table metadata")?;
        }
        Ok(())
    }
}

fn validate_idempotency_key_shape(value: &str) -> LakeCatResult<()> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 || !bytes.iter().all(u8::is_ascii) {
        return Err(LakeCatError::InvalidArgument(
            "table commit idempotency key must be 1..=128 ASCII characters".to_string(),
        ));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Err(LakeCatError::InvalidArgument(
            "table commit idempotency key may only contain A-Z, a-z, 0-9, '-', '_', '.', or ':'"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_idempotency_request_hash_shape(value: &str) -> LakeCatResult<()> {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return Err(LakeCatError::InvalidArgument(
            "table commit idempotency request hash must be full SHA-256 evidence".to_string(),
        ));
    };
    if digest.len() != 64 || !digest.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(LakeCatError::InvalidArgument(
            "table commit idempotency request hash must be full SHA-256 evidence".to_string(),
        ));
    }
    Ok(())
}

fn validate_outbox_event_id_shape(value: &str) -> LakeCatResult<()> {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return Err(LakeCatError::InvalidArgument(
            "outbox event id must be full SHA-256 evidence".to_string(),
        ));
    };
    if digest.len() != 64 || !digest.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(LakeCatError::InvalidArgument(
            "outbox event id must be full SHA-256 evidence".to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableCommitRecord {
    pub table: TableIdent,
    pub previous_metadata_location: Option<String>,
    pub new_metadata_location: Option<String>,
    pub sequence_number: u64,
    pub principal: Principal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_version: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_hash: Option<String>,
    pub request_hash: String,
    pub response_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key_sha256: Option<String>,
    pub committed_at: DateTime<Utc>,
}

impl TableCommitRecord {
    pub fn validate_for_table(&self, ident: &TableIdent) -> LakeCatResult<()> {
        if &self.table != ident {
            return Err(LakeCatError::Internal(
                "table commit record table does not match requested table".to_string(),
            ));
        }
        if self.sequence_number == 0 {
            return Err(LakeCatError::Internal(
                "table commit record sequence number must be positive".to_string(),
            ));
        }
        if self
            .previous_metadata_location
            .as_deref()
            .is_some_and(|location| location.trim().is_empty())
        {
            return Err(LakeCatError::Internal(
                "table commit record previous metadata location must not be empty when present"
                    .to_string(),
            ));
        }
        if self
            .new_metadata_location
            .as_deref()
            .is_some_and(|location| location.trim().is_empty())
        {
            return Err(LakeCatError::Internal(
                "table commit record new metadata location must not be empty when present"
                    .to_string(),
            ));
        }
        let Some(format_version) = self.format_version else {
            return Err(LakeCatError::Internal(
                "table commit record format version must be present".to_string(),
            ));
        };
        if format_version <= 0 {
            return Err(LakeCatError::Internal(
                "table commit record format version must be positive".to_string(),
            ));
        }
        let Some(snapshot_id) = self.snapshot_id else {
            return Err(LakeCatError::Internal(
                "table commit record snapshot id must be present".to_string(),
            ));
        };
        if snapshot_id < 0 {
            return Err(LakeCatError::Internal(
                "table commit record snapshot id must be non-negative".to_string(),
            ));
        }
        validate_sha256_evidence(
            &self.request_hash,
            "table commit record request hash must be full SHA-256 evidence",
        )?;
        validate_sha256_evidence(
            &self.response_hash,
            "table commit record response hash must be full SHA-256 evidence",
        )?;
        if let Some(idempotency_key_sha256) = self.idempotency_key_sha256.as_deref() {
            validate_sha256_evidence(
                idempotency_key_sha256,
                "table commit record idempotency key hash must be full SHA-256 evidence",
            )?;
        }
        if let Some(policy_hash) = self.policy_hash.as_deref() {
            validate_sha256_evidence(
                policy_hash,
                "table commit record policy hash must be full SHA-256 evidence",
            )?;
        }
        Ok(())
    }
}

fn validate_sha256_evidence(value: &str, message: &str) -> LakeCatResult<()> {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return Err(LakeCatError::Internal(message.to_string()));
    };
    if digest.len() != 64 || !digest.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(LakeCatError::Internal(message.to_string()));
    }
    Ok(())
}

fn table_response_hash(table: &TableRecord) -> LakeCatResult<String> {
    let value = serde_json::to_value(table).map_err(|err| {
        LakeCatError::Internal(format!(
            "failed to encode table commit response hash: {err}"
        ))
    })?;
    content_hash_json(&value)
}

fn validate_table_metadata_format_version(metadata: &Value, label: &str) -> LakeCatResult<()> {
    let Some(format_version) = table_metadata_format_version(metadata) else {
        return Err(LakeCatError::InvalidArgument(format!(
            "{label} format-version must be present"
        )));
    };
    if format_version <= 0 {
        return Err(LakeCatError::InvalidArgument(format!(
            "{label} format-version must be positive"
        )));
    }
    Ok(())
}

fn table_metadata_format_version(metadata: &Value) -> Option<i32> {
    metadata
        .get("format-version")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn table_commit_format_version(table: &TableRecord) -> Option<i32> {
    table_metadata_format_version(&table.metadata)
}

fn table_commit_snapshot_id(table: &TableRecord) -> Option<i64> {
    Some(
        table
            .metadata
            .get("current-snapshot-id")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    )
}

fn table_commit_policy_hash(authorization_receipt: Option<&Value>) -> Option<String> {
    authorization_receipt
        .and_then(|receipt| receipt.get("policy_hash"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
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
        if let Some(endpoint_url) = self.endpoint_url.as_deref() {
            validate_server_endpoint_url(endpoint_url)?;
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
        if let Some(storage_root) = self.storage_root.as_deref() {
            validate_warehouse_storage_root_path(storage_root)?;
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
    #[serde(default = "default_view_version")]
    pub view_version: u64,
    pub sql: String,
    pub dialect: String,
    pub schema_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<ViewColumnRecord>,
    pub properties: BTreeMap<String, String>,
    pub created: AuditStamp,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViewColumnRecord {
    pub name: String,
    pub data_type: Value,
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
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
            view_version: 1,
            sql: sql.into(),
            dialect: dialect.into(),
            schema_version,
            columns: Vec::new(),
            properties,
            updated_at: created.at,
            created,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn with_columns(mut self, columns: Vec<ViewColumnRecord>) -> LakeCatResult<Self> {
        self.columns = columns;
        self.validate()?;
        Ok(self)
    }

    pub fn with_next_version(self, previous: Option<&ViewRecord>) -> LakeCatResult<Self> {
        self.with_next_version_after_history(previous, previous.map(|view| view.view_version))
    }

    pub fn with_next_version_after_history(
        mut self,
        previous: Option<&ViewRecord>,
        latest_receipt_version: Option<u64>,
    ) -> LakeCatResult<Self> {
        let base_version = previous
            .map(|view| view.view_version)
            .into_iter()
            .chain(latest_receipt_version)
            .max()
            .unwrap_or(0);
        self.view_version = base_version
            .checked_add(1)
            .ok_or_else(|| LakeCatError::InvalidArgument("view version overflow".to_string()))?;
        if let Some(previous) = previous {
            self.created = previous.created.clone();
        }
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> LakeCatResult<()> {
        if self.view_version == 0 {
            return Err(LakeCatError::InvalidArgument(
                "view version must be greater than zero".to_string(),
            ));
        }
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
        let mut column_names = BTreeSet::new();
        for column in &self.columns {
            column.validate()?;
            if !column_names.insert(column.name.as_str()) {
                return Err(LakeCatError::InvalidArgument(format!(
                    "view column appears more than once: {}",
                    column.name
                )));
            }
        }
        validate_public_config(&self.properties)?;
        Ok(())
    }
}

fn default_view_version() -> u64 {
    1
}

fn validate_expected_view_version(expected: u64) -> LakeCatResult<()> {
    if expected == 0 {
        return Err(LakeCatError::InvalidArgument(
            "expected view version must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

fn require_expected_view_version(
    previous: Option<&ViewRecord>,
    expected: u64,
) -> LakeCatResult<()> {
    let Some(previous) = previous else {
        return Err(LakeCatError::Conflict(format!(
            "view expected version {expected} but no current view exists"
        )));
    };
    if previous.view_version != expected {
        return Err(LakeCatError::Conflict(format!(
            "view expected version {expected} but current version is {}",
            previous.view_version
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ViewVersionOperation {
    Upsert,
    Drop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct ViewVersionReceipt {
    pub stable_id: String,
    pub warehouse: WarehouseName,
    pub namespace: Namespace,
    pub name: TableName,
    pub view_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_view_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_receipt_hash: Option<String>,
    pub operation: ViewVersionOperation,
    pub view_hash: String,
    pub principal: Principal,
    pub recorded_at: DateTime<Utc>,
}

impl ViewVersionReceipt {
    fn upsert(
        previous_view_version: Option<u64>,
        previous_receipt_hash: Option<String>,
        view: &ViewRecord,
        principal: Principal,
    ) -> LakeCatResult<Self> {
        Ok(Self {
            stable_id: view_stable_id(view),
            warehouse: view.warehouse.clone(),
            namespace: view.namespace.clone(),
            name: view.name.clone(),
            view_version: view.view_version,
            previous_view_version,
            previous_receipt_hash,
            operation: ViewVersionOperation::Upsert,
            view_hash: content_hash_json(&serde_json::to_value(view).map_err(|err| {
                LakeCatError::Internal(format!("failed to serialize view receipt: {err}"))
            })?)?,
            principal,
            recorded_at: Utc::now(),
        })
    }

    fn drop(
        view: &ViewRecord,
        previous_receipt_hash: Option<String>,
        principal: Principal,
    ) -> LakeCatResult<Self> {
        Ok(Self {
            stable_id: view_stable_id(view),
            warehouse: view.warehouse.clone(),
            namespace: view.namespace.clone(),
            name: view.name.clone(),
            view_version: view.view_version,
            previous_view_version: Some(view.view_version),
            previous_receipt_hash,
            operation: ViewVersionOperation::Drop,
            view_hash: content_hash_json(&serde_json::to_value(view).map_err(|err| {
                LakeCatError::Internal(format!("failed to serialize view receipt: {err}"))
            })?)?,
            principal,
            recorded_at: Utc::now(),
        })
    }

    pub fn validate(&self) -> LakeCatResult<()> {
        let expected_stable_id = format!(
            "lakecat:view:{}:{}:{}",
            self.warehouse.as_str(),
            self.namespace.path(),
            self.name.as_str()
        );
        if self.stable_id != expected_stable_id {
            return Err(LakeCatError::InvalidArgument(
                "view receipt stable id does not match receipt identity".to_string(),
            ));
        }
        if self.view_version == 0 {
            return Err(LakeCatError::InvalidArgument(
                "view receipt version must be greater than zero".to_string(),
            ));
        }
        match self.operation {
            ViewVersionOperation::Upsert => {
                if let Some(previous) = self.previous_view_version
                    && previous >= self.view_version
                {
                    return Err(LakeCatError::InvalidArgument(
                        "view upsert receipt previous version must be less than receipt version"
                            .to_string(),
                    ));
                }
            }
            ViewVersionOperation::Drop => {
                if self.previous_view_version != Some(self.view_version) {
                    return Err(LakeCatError::InvalidArgument(
                        "view drop receipt previous version must equal receipt version".to_string(),
                    ));
                }
            }
        }
        validate_sha256_evidence(
            &self.view_hash,
            "view receipt hash must be a SHA-256 digest",
        )?;
        if let Some(previous_receipt_hash) = self.previous_receipt_hash.as_deref() {
            validate_sha256_evidence(
                previous_receipt_hash,
                "previous view receipt hash must be a SHA-256 digest",
            )?;
        }
        Ok(())
    }
}

fn view_receipt_hash(receipt: &ViewVersionReceipt) -> LakeCatResult<String> {
    content_hash_json(&serde_json::to_value(receipt).map_err(|err| {
        LakeCatError::Internal(format!("failed to serialize view receipt: {err}"))
    })?)
}

fn latest_view_receipt_evidence<'a>(
    receipts: impl Iterator<Item = &'a ViewVersionReceipt>,
) -> LakeCatResult<Option<(u64, String)>> {
    let receipts = receipts.collect::<Vec<_>>();
    let receipt_chain = receipts
        .iter()
        .map(|receipt| (*receipt).clone())
        .collect::<Vec<_>>();
    validate_view_receipt_chains(&receipt_chain)?;
    receipts
        .into_iter()
        .max_by(|left, right| {
            left.view_version
                .cmp(&right.view_version)
                .then_with(|| left.recorded_at.cmp(&right.recorded_at))
                .then_with(|| {
                    view_version_operation_order(&left.operation)
                        .cmp(&view_version_operation_order(&right.operation))
                })
        })
        .map(|receipt| {
            receipt.validate()?;
            Ok((receipt.view_version, view_receipt_hash(receipt)?))
        })
        .transpose()
}

fn latest_view_receipt_hash<'a>(
    receipts: impl Iterator<Item = &'a ViewVersionReceipt>,
) -> LakeCatResult<Option<String>> {
    latest_view_receipt_evidence(receipts).map(|evidence| evidence.map(|(_, hash)| hash))
}

fn validate_view_receipt_chains(receipts: &[ViewVersionReceipt]) -> LakeCatResult<()> {
    let mut grouped = BTreeMap::<&str, Vec<&ViewVersionReceipt>>::new();
    for receipt in receipts {
        grouped
            .entry(receipt.stable_id.as_str())
            .or_default()
            .push(receipt);
    }

    for mut chain in grouped.into_values() {
        chain.sort_by(|left, right| {
            left.view_version
                .cmp(&right.view_version)
                .then_with(|| left.recorded_at.cmp(&right.recorded_at))
                .then_with(|| {
                    view_version_operation_order(&left.operation)
                        .cmp(&view_version_operation_order(&right.operation))
                })
        });
        for (index, receipt) in chain.iter().enumerate() {
            receipt.validate()?;
            let Some(previous) = index.checked_sub(1).and_then(|index| chain.get(index)) else {
                if receipt.operation != ViewVersionOperation::Upsert
                    || receipt.view_version != 1
                    || receipt.previous_view_version.is_some()
                    || receipt.previous_receipt_hash.is_some()
                {
                    return Err(LakeCatError::Internal(
                        "view receipt chain must begin with version 1 upsert without previous links"
                            .to_string(),
                    ));
                }
                continue;
            };
            let previous_hash = view_receipt_hash(previous)?;
            if receipt.previous_receipt_hash.as_deref() != Some(previous_hash.as_str()) {
                return Err(LakeCatError::Internal(
                    "view receipt chain previous links must match the prior receipt".to_string(),
                ));
            }
            match receipt.operation {
                ViewVersionOperation::Upsert => {
                    if receipt.previous_view_version != Some(previous.view_version)
                        || receipt.view_version != previous.view_version.saturating_add(1)
                    {
                        return Err(LakeCatError::Internal(
                            "view receipt chain upsert transition is invalid".to_string(),
                        ));
                    }
                }
                ViewVersionOperation::Drop => {
                    if receipt.previous_view_version != Some(previous.view_version)
                        || receipt.view_version != previous.view_version
                    {
                        return Err(LakeCatError::Internal(
                            "view receipt chain drop transition is invalid".to_string(),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_view_receipt_scope(
    receipt: &ViewVersionReceipt,
    warehouse: &WarehouseName,
    namespace: &Namespace,
    view: Option<&TableName>,
) -> LakeCatResult<()> {
    if receipt.warehouse != *warehouse || receipt.namespace != *namespace {
        return Err(LakeCatError::Internal(
            "view receipt row scope does not match receipt identity".to_string(),
        ));
    }
    if let Some(view) = view
        && receipt.name != *view
    {
        return Err(LakeCatError::Internal(
            "view receipt row scope does not match receipt identity".to_string(),
        ));
    }
    Ok(())
}

fn validate_view_record_scope(
    record: &ViewRecord,
    warehouse: &WarehouseName,
    namespace: &Namespace,
    view: &TableName,
) -> LakeCatResult<()> {
    record.validate()?;
    if record.warehouse != *warehouse || record.namespace != *namespace || record.name != *view {
        return Err(LakeCatError::Internal(
            "view record row scope does not match view identity".to_string(),
        ));
    }
    Ok(())
}

fn view_version_operation_order(operation: &ViewVersionOperation) -> u8 {
    match operation {
        ViewVersionOperation::Upsert => 0,
        ViewVersionOperation::Drop => 1,
    }
}

impl ViewColumnRecord {
    pub fn validate(&self) -> LakeCatResult<()> {
        if self.name.trim().is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "view column name must not be empty".to_string(),
            ));
        }
        if self.data_type.is_null() {
            return Err(LakeCatError::InvalidArgument(format!(
                "view column {} data type must not be null",
                self.name
            )));
        }
        if let Some(comment) = self.comment.as_deref()
            && comment.trim().is_empty()
        {
            return Err(LakeCatError::InvalidArgument(format!(
                "view column {} comment must not be empty",
                self.name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoftDeleteRecord {
    pub table: TableIdent,
    pub metadata_location: Option<String>,
    pub version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_version: Option<i32>,
    pub principal: Principal,
    pub authorization_receipt: Option<Value>,
    pub deleted_at: DateTime<Utc>,
}

impl SoftDeleteRecord {
    pub fn validate_for_table(&self, ident: &TableIdent, table: &TableRecord) -> LakeCatResult<()> {
        table.validate()?;
        if self.table != *ident {
            return Err(LakeCatError::InvalidArgument(
                "soft-delete record table does not match requested table".to_string(),
            ));
        }
        if table.ident != *ident {
            return Err(LakeCatError::InvalidArgument(
                "soft-delete table record does not match requested table".to_string(),
            ));
        }
        if self
            .metadata_location
            .as_deref()
            .is_some_and(|location| location.trim().is_empty())
        {
            return Err(LakeCatError::InvalidArgument(
                "soft-delete metadata location must not be empty when present".to_string(),
            ));
        }
        if self.metadata_location != table.metadata_location {
            return Err(LakeCatError::InvalidArgument(
                "soft-delete metadata location does not match table record".to_string(),
            ));
        }
        if self.version != table.version {
            return Err(LakeCatError::InvalidArgument(
                "soft-delete version does not match table record".to_string(),
            ));
        }
        let Some(format_version) = self.format_version else {
            return Err(LakeCatError::InvalidArgument(
                "soft-delete format version must be present".to_string(),
            ));
        };
        if format_version <= 0 {
            return Err(LakeCatError::InvalidArgument(
                "soft-delete format version must be positive".to_string(),
            ));
        }
        if self.format_version != table_commit_format_version(table) {
            return Err(LakeCatError::InvalidArgument(
                "soft-delete format version does not match table record".to_string(),
            ));
        }
        if let Some(receipt) = &self.authorization_receipt
            && !receipt.is_object()
        {
            return Err(LakeCatError::InvalidArgument(
                "soft-delete authorization receipt must be a JSON object when present".to_string(),
            ));
        }
        Ok(())
    }
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
        validate_location_prefix_path(&location_prefix)?;
        validate_location_prefix_provider(&location_prefix, provider)?;
        validate_issuance_mode_provider(issuance_mode, provider, &location_prefix)?;
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
        validate_storage_profile_public_config(&public_config)?;
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
        validate_profile_id(&self.profile_id)?;
        validate_location_prefix_path(&self.location_prefix)?;
        validate_location_prefix_provider(&self.location_prefix, self.provider)?;
        validate_issuance_mode_provider(self.issuance_mode, self.provider, &self.location_prefix)?;
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
        validate_storage_profile_public_config(&self.public_config)?;
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

impl OutboxEvent {
    pub fn validate_pending(&self) -> LakeCatResult<()> {
        let event_id_hash = content_hash_bytes(self.event_id.as_bytes());
        let event_type_hash = content_hash_bytes(self.event_type.as_bytes());
        let payload_hash = content_hash_json(&self.payload)?;
        if self.delivered_at.is_some() {
            return Err(LakeCatError::Internal(format!(
                "pending outbox event must not already be delivered; event-id-hash={event_id_hash}; event-type-hash={event_type_hash}; payload-hash={payload_hash}"
            )));
        }
        if self.sink.trim().is_empty() {
            return Err(LakeCatError::Internal(format!(
                "pending outbox event sink must not be empty; event-id-hash={event_id_hash}; event-type-hash={event_type_hash}; payload-hash={payload_hash}"
            )));
        }
        if self.event_type.trim().is_empty() {
            return Err(LakeCatError::Internal(format!(
                "pending outbox event type must not be empty; event-id-hash={event_id_hash}; event-type-hash={event_type_hash}; payload-hash={payload_hash}"
            )));
        }
        let payload_event_type = self
            .payload
            .get("event-type")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                LakeCatError::Internal(format!(
                    "pending outbox payload missing event-type; event-id-hash={event_id_hash}; event-type-hash={event_type_hash}; payload-hash={payload_hash}"
                ))
            })?;
        if payload_event_type.trim().is_empty() {
            return Err(LakeCatError::Internal(format!(
                "pending outbox payload event-type must not be empty; event-id-hash={event_id_hash}; event-type-hash={event_type_hash}; payload-event-type-hash={}; payload-hash={payload_hash}",
                content_hash_bytes(payload_event_type.as_bytes())
            )));
        }
        if payload_event_type != self.event_type {
            return Err(LakeCatError::Internal(format!(
                "pending outbox event type does not match payload; event-id-hash={event_id_hash}; event-type-hash={}; payload-event-type-hash={}; payload-hash={payload_hash}",
                event_type_hash,
                content_hash_bytes(payload_event_type.as_bytes())
            )));
        }
        if self.event_id != payload_hash {
            return Err(LakeCatError::Internal(format!(
                "pending outbox event id does not match payload hash; event-id-hash={event_id_hash}; event-type-hash={event_type_hash}; payload-hash={payload_hash}"
            )));
        }
        Ok(())
    }
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

    pub fn validate(&self) -> LakeCatResult<()> {
        validate_policy_id(&self.policy_id)?;
        if self.table.is_some() && self.namespace.is_none() {
            return Err(LakeCatError::InvalidArgument(
                "table-scoped policy binding requires namespace".to_string(),
            ));
        }
        Ok(())
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

    pub fn validate_recordable(&self) -> LakeCatResult<()> {
        if self.event_type.trim().is_empty() {
            return Err(LakeCatError::InvalidArgument(
                "audit event type must not be empty".to_string(),
            ));
        }
        let payload_event_type = self
            .payload
            .get("event-type")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                LakeCatError::InvalidArgument("audit event payload missing event-type".to_string())
            })?;
        if payload_event_type != self.event_type {
            return Err(LakeCatError::InvalidArgument(
                "audit event type does not match payload".to_string(),
            ));
        }
        let request_hash = self.request_hash.as_deref().ok_or_else(|| {
            LakeCatError::InvalidArgument("audit event request hash is required".to_string())
        })?;
        let payload_hash = content_hash_json(&self.payload)?;
        if request_hash != payload_hash {
            return Err(LakeCatError::InvalidArgument(
                "audit event request hash does not match payload".to_string(),
            ));
        }
        validate_audit_payload_authorization_principal(&self.payload, &self.principal)?;
        if let Some(table) = &self.table {
            validate_audit_payload_table_scope(&self.payload, table)?;
        }
        Ok(())
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
    view_version_receipts: Vec<ViewVersionReceipt>,
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

    async fn load_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        let state = self.state.read().await;
        state
            .namespaces
            .get(warehouse.as_str())
            .and_then(|set| set.get(namespace))
            .cloned()
            .ok_or_else(|| namespace_not_found(namespace))
    }

    async fn drop_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Namespace> {
        let mut state = self.state.write().await;
        if !state
            .namespaces
            .get(warehouse.as_str())
            .is_some_and(|set| set.contains(namespace))
        {
            return Err(namespace_not_found(namespace));
        }
        if state
            .tables
            .values()
            .any(|table| table.ident.warehouse == *warehouse && table.ident.namespace == *namespace)
        {
            return Err(namespace_not_empty(namespace, "tables"));
        }
        if state
            .views
            .values()
            .any(|view| view.warehouse == *warehouse && view.namespace == *namespace)
        {
            return Err(namespace_not_empty(namespace, "views"));
        }
        if state.policy_bindings.values().any(|binding| {
            binding.warehouse == *warehouse && binding.namespace.as_ref() == Some(namespace)
        }) {
            return Err(namespace_not_empty(namespace, "policy bindings"));
        }
        let namespaces = state
            .namespaces
            .get_mut(warehouse.as_str())
            .ok_or_else(|| namespace_not_found(namespace))?;
        namespaces.remove(namespace);
        Ok(namespace.clone())
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
        table.validate()?;
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
        commit.validate()?;
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
                return Err(metadata_pointer_conflict(
                    ident,
                    commit.expected_previous_metadata_location.as_deref(),
                    previous_metadata_location.as_deref(),
                ));
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
            format_version: table_commit_format_version(&table),
            snapshot_id: table_commit_snapshot_id(&table),
            policy_hash: table_commit_policy_hash(commit.authorization_receipt.as_ref()),
            request_hash,
            response_hash: table_response_hash(&table)?,
            idempotency_key_sha256,
            committed_at,
        };
        let replay_request_hash = record.request_hash.clone();
        let audit_payload = serde_json::json!({
            "event-type": "table.commit",
            "table": ident.clone(),
            "commit": &record,
            "authorization-receipt": commit.authorization_receipt,
        });
        let audit_event_id = content_hash_json(&audit_payload)?;
        let outbox_payload = serde_json::json!({
            "audit-event-id": audit_event_id,
            "event-type": "table.commit",
            "table": ident.clone(),
            "commit": &record,
            "authorization-receipt": audit_payload["authorization-receipt"].clone(),
        });
        let outbox_event = outbox_event_from_payload(&outbox_payload, committed_at)?;
        state.audit_events.push(CatalogAuditEvent {
            event_type: "table.commit".to_string(),
            table: Some(ident.clone()),
            principal: commit.principal.clone(),
            request_hash: Some(record.request_hash.clone()),
            payload: audit_payload,
            created_at: committed_at,
        });
        state.outbox_events.push(outbox_event);
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

    async fn replay_table_commit(
        &self,
        ident: &TableIdent,
        idempotency_key: &str,
        idempotency_request_hash: &str,
    ) -> LakeCatResult<Option<TableRecord>> {
        validate_idempotency_key_shape(idempotency_key)?;
        validate_idempotency_request_hash_shape(idempotency_request_hash)?;
        let state = self.state.read().await;
        let idem_key = format!("{}:{idempotency_key}", ident.stable_id());
        let Some(replay) = state.idempotency.get(&idem_key) else {
            return Ok(None);
        };
        if replay.request_hash != idempotency_request_hash {
            return Err(LakeCatError::Conflict(format!(
                "idempotency key reused with different commit request for {}",
                ident.stable_id()
            )));
        }
        Ok(Some(replay.response.clone()))
    }

    async fn table_commit_records(
        &self,
        ident: &TableIdent,
        start_version: u64,
        end_version: Option<u64>,
    ) -> LakeCatResult<Vec<TableCommitRecord>> {
        let state = self.state.read().await;
        state
            .commits
            .iter()
            .filter(|commit| &commit.table == ident)
            .filter(|commit| commit.sequence_number >= start_version)
            .filter(|commit| end_version.is_none_or(|end| commit.sequence_number <= end))
            .map(|commit| {
                commit.validate_for_table(ident)?;
                Ok(commit.clone())
            })
            .collect()
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
        for server in &servers {
            server.validate()?;
        }
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
        for project in &projects {
            project.validate()?;
        }
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
        let warehouse = state
            .warehouses
            .get(warehouse.as_str())
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "warehouse",
                name: warehouse.as_str().to_string(),
            })?;
        warehouse.validate()?;
        Ok(warehouse)
    }

    async fn list_warehouses(&self) -> LakeCatResult<Vec<WarehouseRecord>> {
        let state = self.state.read().await;
        let mut warehouses = state.warehouses.values().cloned().collect::<Vec<_>>();
        warehouses.sort_by(|left, right| left.warehouse.as_str().cmp(right.warehouse.as_str()));
        for warehouse in &warehouses {
            warehouse.validate()?;
        }
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
        for warehouse in &warehouses {
            warehouse.validate()?;
        }
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
        let record = SoftDeleteRecord {
            table: ident.clone(),
            metadata_location: table.metadata_location.clone(),
            version: table.version,
            format_version: table_commit_format_version(&table),
            principal,
            authorization_receipt,
            deleted_at: Utc::now(),
        };
        record.validate_for_table(ident, &table)?;
        state.soft_deletes.insert(key, record);
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
        let Some(record) = state.soft_deletes.get(&key) else {
            return Err(LakeCatError::NotFound {
                object: "soft-deleted table",
                name: ident.stable_id(),
            });
        };
        let table = state
            .tables
            .get(&key)
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })?;
        record.validate_for_table(ident, &table)?;
        state.soft_deletes.remove(&key);
        Ok(table)
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
        for profile in &profiles {
            profile.validate()?;
        }
        Ok(profiles)
    }

    async fn upsert_view(&self, view: ViewRecord) -> LakeCatResult<ViewRecord> {
        view.validate()?;
        let mut state = self.state.write().await;
        let view_key = view_key(&view);
        let principal = view.created.principal.clone();
        let previous = state.views.get(&view_key);
        let latest_receipt = latest_view_receipt_evidence(
            state
                .view_version_receipts
                .iter()
                .filter(|receipt| receipt.stable_id == view_stable_id(&view)),
        )?;
        let latest_receipt_version = latest_receipt
            .as_ref()
            .map(|(view_version, _)| *view_version);
        let previous_receipt_hash = latest_receipt.map(|(_, receipt_hash)| receipt_hash);
        let previous_view_version = previous
            .map(|view| view.view_version)
            .or(latest_receipt_version);
        let view = view.with_next_version_after_history(previous, latest_receipt_version)?;
        let receipt = ViewVersionReceipt::upsert(
            previous_view_version,
            previous_receipt_hash,
            &view,
            principal,
        )?;
        state.views.insert(view_key, view.clone());
        state.view_version_receipts.push(receipt);
        Ok(view)
    }

    async fn upsert_view_if_version(
        &self,
        view: ViewRecord,
        expected_view_version: Option<u64>,
    ) -> LakeCatResult<ViewRecord> {
        view.validate()?;
        if let Some(expected) = expected_view_version {
            validate_expected_view_version(expected)?;
        }
        let mut state = self.state.write().await;
        let view_key = view_key(&view);
        let principal = view.created.principal.clone();
        let previous = state.views.get(&view_key);
        if let Some(expected) = expected_view_version {
            require_expected_view_version(previous, expected)?;
        }
        let latest_receipt = latest_view_receipt_evidence(
            state
                .view_version_receipts
                .iter()
                .filter(|receipt| receipt.stable_id == view_stable_id(&view)),
        )?;
        let latest_receipt_version = latest_receipt
            .as_ref()
            .map(|(view_version, _)| *view_version);
        let previous_receipt_hash = latest_receipt.map(|(_, receipt_hash)| receipt_hash);
        let previous_view_version = previous
            .map(|view| view.view_version)
            .or(latest_receipt_version);
        let view = view.with_next_version_after_history(previous, latest_receipt_version)?;
        let receipt = ViewVersionReceipt::upsert(
            previous_view_version,
            previous_receipt_hash,
            &view,
            principal,
        )?;
        state.views.insert(view_key, view.clone());
        state.view_version_receipts.push(receipt);
        Ok(view)
    }

    async fn list_view_version_receipts(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        let state = self.state.read().await;
        let receipts = state
            .view_version_receipts
            .iter()
            .filter(|receipt| {
                receipt.warehouse == *warehouse
                    && receipt.namespace == *namespace
                    && receipt.name == *view
            })
            .cloned()
            .collect::<Vec<_>>();
        validate_view_receipt_chains(&receipts)?;
        Ok(receipts)
    }

    async fn list_namespace_view_version_receipts(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        let state = self.state.read().await;
        let receipts = state
            .view_version_receipts
            .iter()
            .filter(|receipt| receipt.warehouse == *warehouse && receipt.namespace == *namespace)
            .cloned()
            .collect::<Vec<_>>();
        validate_view_receipt_chains(&receipts)?;
        Ok(receipts)
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
            .and_then(|record| {
                validate_view_record_scope(&record, warehouse, namespace, view)?;
                Ok(record)
            })
    }

    async fn drop_view(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
        principal: Principal,
    ) -> LakeCatResult<ViewRecord> {
        self.drop_view_if_version(warehouse, namespace, view, principal, None)
            .await
    }

    async fn drop_view_if_version(
        &self,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
        principal: Principal,
        expected_view_version: Option<u64>,
    ) -> LakeCatResult<ViewRecord> {
        if let Some(expected) = expected_view_version {
            validate_expected_view_version(expected)?;
        }
        let mut state = self.state.write().await;
        let view_key = view_key_parts(warehouse, namespace, view);
        let current = state
            .views
            .get(&view_key)
            .ok_or_else(|| LakeCatError::NotFound {
                object: "view",
                name: view.as_str().to_string(),
            })?;
        validate_view_record_scope(current, warehouse, namespace, view)?;
        if let Some(expected) = expected_view_version {
            require_expected_view_version(Some(current), expected)?;
        }
        let record = state.views.remove(&view_key).ok_or_else(|| {
            LakeCatError::Internal("view disappeared during guarded drop".to_string())
        })?;
        let previous_receipt_hash = latest_view_receipt_hash(
            state
                .view_version_receipts
                .iter()
                .filter(|receipt| receipt.stable_id == view_stable_id(&record)),
        )?;
        let receipt = ViewVersionReceipt::drop(&record, previous_receipt_hash, principal)?;
        state.view_version_receipts.push(receipt);
        Ok(record)
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
        for view in &views {
            validate_view_record_scope(view, warehouse, namespace, &view.name)?;
        }
        Ok(views)
    }

    async fn storage_profile_for_table(
        &self,
        table: &TableRecord,
    ) -> LakeCatResult<StorageProfile> {
        let state = self.state.read().await;
        let profiles = state.storage_profiles.values().collect::<Vec<_>>();
        for profile in &profiles {
            profile.validate()?;
        }
        Ok(storage_profile_match(profiles.into_iter(), table)?
            .unwrap_or_else(|| StorageProfile::inferred_for_table(table)))
    }

    async fn upsert_policy_binding(&self, binding: PolicyBinding) -> LakeCatResult<PolicyBinding> {
        binding.validate()?;
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
        for binding in &bindings {
            binding.validate()?;
        }
        Ok(bindings)
    }

    async fn policy_bindings_for_table(
        &self,
        table: &TableIdent,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        let state = self.state.read().await;
        let bindings = state.policy_bindings.values().collect::<Vec<_>>();
        for binding in &bindings {
            binding.validate()?;
        }
        Ok(policy_bindings_for_table(bindings.into_iter(), table))
    }

    async fn record_audit_event(&self, event: CatalogAuditEvent) -> LakeCatResult<()> {
        event.validate_recordable()?;
        let event_id = audit_event_id(&event)?;
        let outbox_payload = audit_outbox_payload(&event_id, &event);
        let outbox_event = outbox_event_from_payload(&outbox_payload, event.created_at)?;
        let mut state = self.state.write().await;
        if state
            .outbox_events
            .iter()
            .any(|candidate| candidate.event_id == outbox_event.event_id)
        {
            return Err(LakeCatError::Internal(
                "duplicate audit event id would duplicate outbox replay evidence".to_string(),
            ));
        }
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
        let mut events = state
            .outbox_events
            .iter()
            .filter(|event| event.delivered_at.is_none())
            .filter(|event| sink.is_none_or(|sink| event.sink == sink))
            .cloned()
            .collect::<Vec<_>>();
        events.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        events.truncate(limit);
        for event in &events {
            event.validate_pending()?;
        }
        Ok(events)
    }

    async fn mark_outbox_delivered(&self, event_ids: &[String]) -> LakeCatResult<usize> {
        if event_ids.is_empty() {
            return Ok(0);
        }
        for event_id in event_ids {
            validate_outbox_event_id_shape(event_id)?;
        }
        let event_ids = event_ids.iter().collect::<BTreeSet<_>>();
        let mut state = self.state.write().await;
        let delivered_at = Utc::now();
        let mut delivered = 0usize;
        for event in &mut state.outbox_events {
            if event.delivered_at.is_none() && event_ids.contains(&event.event_id) {
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

fn metadata_pointer_conflict(
    ident: &TableIdent,
    expected_metadata_location: Option<&str>,
    actual_metadata_location: Option<&str>,
) -> LakeCatError {
    let expected_hash = optional_location_hash(expected_metadata_location);
    let actual_hash = optional_location_hash(actual_metadata_location);
    LakeCatError::Conflict(format!(
        "metadata pointer changed for {}; expected-metadata-location-hash={expected_hash}; actual-metadata-location-hash={actual_hash}",
        ident.stable_id()
    ))
}

fn optional_location_hash(location: Option<&str>) -> String {
    location
        .map(|location| content_hash_bytes(location.as_bytes()))
        .unwrap_or_else(|| "null".to_string())
}

#[cfg(feature = "turso-local")]
fn validate_table_record_identity(record: &TableRecord, ident: &TableIdent) -> LakeCatResult<()> {
    record.validate()?;
    if record.ident != *ident {
        return Err(LakeCatError::Internal(
            "table record row scope does not match requested table".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_table_record_scope(
    record: &TableRecord,
    ident: &TableIdent,
    row_table_key: &str,
    row_warehouse: &str,
    row_namespace_path: &str,
    row_table_name: &str,
) -> LakeCatResult<()> {
    validate_table_record_identity(record, ident)?;
    if row_table_key != table_key(ident)
        || row_warehouse != ident.warehouse.as_str()
        || row_namespace_path != ident.namespace.path()
        || row_table_name != ident.name.as_str()
    {
        return Err(LakeCatError::Internal(
            "table record row scope does not match requested table".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_idempotency_record_table_key(
    row_table_key: &str,
    ident: &TableIdent,
) -> LakeCatResult<()> {
    if row_table_key != table_key(ident) {
        return Err(LakeCatError::Internal(
            "idempotency record row scope does not match requested table".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_namespace_scope(
    namespace: &Namespace,
    expected_warehouse: &WarehouseName,
    row_warehouse: &WarehouseName,
    row_namespace_path: &str,
) -> LakeCatResult<()> {
    if row_warehouse != expected_warehouse || namespace.path() != row_namespace_path {
        return Err(LakeCatError::Internal(
            "namespace row scope does not match namespace identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_policy_binding_scope(
    binding: &PolicyBinding,
    warehouse: &WarehouseName,
    policy_id: &str,
    row_namespace_path: Option<&str>,
    row_table_name: Option<&str>,
    row_enforced: bool,
) -> LakeCatResult<()> {
    binding.validate()?;
    let binding_namespace_path = binding.namespace.as_ref().map(Namespace::path);
    let binding_table_name = binding.table.as_ref().map(TableName::as_str);
    if binding.warehouse != *warehouse
        || binding.policy_id != policy_id
        || binding_namespace_path.as_deref() != row_namespace_path
        || binding_table_name != row_table_name
        || binding.enforced != row_enforced
    {
        return Err(LakeCatError::Internal(
            "policy binding row scope does not match binding identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_storage_profile_scope(
    profile: &StorageProfile,
    warehouse: &WarehouseName,
    profile_id: &str,
    row_location_prefix: &str,
    row_provider: &str,
    row_issuance_mode: &str,
) -> LakeCatResult<()> {
    profile.validate()?;
    if profile.warehouse != *warehouse
        || profile.profile_id != profile_id
        || profile.location_prefix != row_location_prefix
        || profile.provider.as_str() != row_provider
        || profile.issuance_mode.as_str() != row_issuance_mode
    {
        return Err(LakeCatError::Internal(
            "storage profile row scope does not match profile identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_server_record_scope(server: &ServerRecord, server_id: &str) -> LakeCatResult<()> {
    server.validate()?;
    if server.server_id != server_id {
        return Err(LakeCatError::Internal(
            "server row scope does not match server identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_project_record_scope(project: &ProjectRecord, project_id: &str) -> LakeCatResult<()> {
    project.validate()?;
    if project.project_id != project_id {
        return Err(LakeCatError::Internal(
            "project row scope does not match project identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_warehouse_record_scope(
    record: &WarehouseRecord,
    warehouse: &WarehouseName,
    row_project_id: &str,
    row_storage_root: Option<&str>,
) -> LakeCatResult<()> {
    record.validate()?;
    if record.warehouse != *warehouse
        || record.project_id != row_project_id
        || record.storage_root.as_deref() != row_storage_root
    {
        return Err(LakeCatError::Internal(
            "warehouse row scope does not match warehouse identity".to_string(),
        ));
    }
    Ok(())
}

fn view_key(view: &ViewRecord) -> String {
    view_key_parts(&view.warehouse, &view.namespace, &view.name)
}

fn view_stable_id(view: &ViewRecord) -> String {
    format!(
        "lakecat:view:{}:{}:{}",
        view.warehouse, view.namespace, view.name
    )
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
    let mut payload = serde_json::Map::new();
    payload.insert(
        "audit-event-id".to_string(),
        Value::String(event_id.to_string()),
    );
    payload.insert(
        "event-type".to_string(),
        Value::String(event.event_type.clone()),
    );
    if let Some(table) = &event.table {
        payload.insert(
            "table".to_string(),
            serde_json::to_value(table).expect("table serializes"),
        );
    }
    payload.insert("payload".to_string(), event.payload.clone());
    Value::Object(payload)
}

fn validate_audit_payload_authorization_principal(
    payload: &Value,
    principal: &Principal,
) -> LakeCatResult<()> {
    let Some(receipt) = payload.get("authorization-receipt") else {
        return Ok(());
    };
    let action = receipt
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "audit event authorization receipt action is required".to_string(),
            )
        })?;
    if action.trim().is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "audit event authorization receipt action must not be empty".to_string(),
        ));
    }
    let Some(receipt_principal) = receipt.get("principal") else {
        return Ok(());
    };
    let receipt_principal = serde_json::from_value::<Principal>(receipt_principal.clone())
        .map_err(|_| {
            LakeCatError::InvalidArgument(
                "audit event authorization receipt principal is malformed".to_string(),
            )
        })?;
    if &receipt_principal != principal {
        return Err(LakeCatError::InvalidArgument(
            "audit event authorization receipt principal does not match event principal"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_audit_payload_table_scope(payload: &Value, table: &TableIdent) -> LakeCatResult<()> {
    let Some(payload_table) = payload.get("table") else {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload missing table scope".to_string(),
        ));
    };
    if payload_table.is_object() {
        let payload_table =
            serde_json::from_value::<TableIdent>(payload_table.clone()).map_err(|_| {
                LakeCatError::InvalidArgument(
                    "audit event payload table scope is malformed".to_string(),
                )
            })?;
        if &payload_table != table {
            return Err(LakeCatError::InvalidArgument(
                "audit event payload table scope does not match event table".to_string(),
            ));
        }
        return Ok(());
    }
    let Some(payload_table_name) = payload_table.as_str() else {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload table scope is malformed".to_string(),
        ));
    };
    if payload_table_name != table.name.as_str() {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload table scope does not match event table".to_string(),
        ));
    }
    let payload_warehouse = payload
        .get("warehouse")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "audit event payload missing warehouse scope for table".to_string(),
            )
        })?;
    if payload_warehouse != table.warehouse.as_str() {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload warehouse scope does not match event table".to_string(),
        ));
    }
    let payload_namespace = payload.get("namespace").ok_or_else(|| {
        LakeCatError::InvalidArgument(
            "audit event payload missing namespace scope for table".to_string(),
        )
    })?;
    if !audit_payload_namespace_matches(payload_namespace, &table.namespace) {
        return Err(LakeCatError::InvalidArgument(
            "audit event payload namespace scope does not match event table".to_string(),
        ));
    }
    Ok(())
}

fn audit_payload_namespace_matches(payload_namespace: &Value, namespace: &Namespace) -> bool {
    if let Some(namespace_path) = payload_namespace.as_str() {
        return namespace_path == namespace.path();
    }
    let Some(parts) = payload_namespace.as_array() else {
        return false;
    };
    let payload_parts = parts.iter().filter_map(Value::as_str).collect::<Vec<_>>();
    payload_parts.len() == parts.len()
        && payload_parts
            .iter()
            .copied()
            .eq(namespace.parts().iter().map(String::as_str))
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

fn namespace_not_found(namespace: &Namespace) -> LakeCatError {
    LakeCatError::NotFound {
        object: "namespace",
        name: namespace.path(),
    }
}

fn namespace_not_empty(namespace: &Namespace, reason: &str) -> LakeCatError {
    LakeCatError::Conflict(format!(
        "cannot drop non-empty namespace {}: contains {reason}",
        namespace.path()
    ))
}

fn storage_profile_match<'a>(
    profiles: impl IntoIterator<Item = &'a StorageProfile>,
    table: &TableRecord,
) -> LakeCatResult<Option<StorageProfile>> {
    let mut best: Option<&StorageProfile> = None;
    for profile in profiles
        .into_iter()
        .filter(|profile| profile.warehouse == table.ident.warehouse)
        .filter(|profile| {
            location_matches_storage_profile_prefix(&table.location, &profile.location_prefix)
        })
    {
        let Some(current) = best else {
            best = Some(profile);
            continue;
        };
        match profile
            .location_prefix
            .len()
            .cmp(&current.location_prefix.len())
        {
            std::cmp::Ordering::Greater => best = Some(profile),
            std::cmp::Ordering::Equal => {
                return Err(LakeCatError::InvalidArgument(format!(
                    "ambiguous storage profile match for {}; location-prefix-hash={}; profile-ids={},{}",
                    table.ident.stable_id(),
                    content_hash_bytes(profile.location_prefix.as_bytes()),
                    current.profile_id,
                    profile.profile_id
                )));
            }
            std::cmp::Ordering::Less => {}
        }
    }
    Ok(best.cloned())
}

fn location_matches_storage_profile_prefix(location: &str, prefix: &str) -> bool {
    if location == prefix {
        return true;
    }
    if prefix.ends_with('/') {
        return location.starts_with(prefix);
    }
    location
        .strip_prefix(prefix)
        .is_some_and(|suffix| suffix.starts_with('/'))
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
            "storage profile id contains unsupported characters; {}",
            storage_profile_id_hash_context(profile_id)
        )));
    }
    Ok(())
}

fn validate_location_prefix_provider(
    location_prefix: &str,
    provider: StorageProvider,
) -> LakeCatResult<()> {
    let detected = StorageProvider::from_location(location_prefix);
    if detected != StorageProvider::Unknown && detected != provider {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile provider '{}' does not match location prefix provider '{}'; {}",
            provider.as_str(),
            detected.as_str(),
            storage_profile_prefix_hash_context(location_prefix)
        )));
    }
    if detected == StorageProvider::Unknown && provider != StorageProvider::Unknown {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile location prefix is not supported by provider '{}'; {}",
            provider.as_str(),
            storage_profile_prefix_hash_context(location_prefix)
        )));
    }
    Ok(())
}

fn validate_location_prefix_path(location_prefix: &str) -> LakeCatResult<()> {
    if location_prefix_has_query_fragment_or_userinfo(location_prefix) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile location prefix must not include query strings, fragments, or userinfo; {}",
            storage_profile_prefix_hash_context(location_prefix)
        )));
    }
    if location_prefix_has_dot_path_segment(location_prefix) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile location prefix must not include dot path segments; {}",
            storage_profile_prefix_hash_context(location_prefix)
        )));
    }
    Ok(())
}

fn validate_warehouse_storage_root_path(storage_root: &str) -> LakeCatResult<()> {
    if location_has_query_fragment_or_userinfo(storage_root) {
        return Err(LakeCatError::InvalidArgument(format!(
            "warehouse storage root must not include query strings, fragments, or userinfo; {}",
            warehouse_storage_root_hash_context(storage_root)
        )));
    }
    if location_has_dot_path_segment(storage_root) {
        return Err(LakeCatError::InvalidArgument(format!(
            "warehouse storage root must not include dot path segments; {}",
            warehouse_storage_root_hash_context(storage_root)
        )));
    }
    Ok(())
}

fn validate_server_endpoint_url(endpoint_url: &str) -> LakeCatResult<()> {
    let url = Url::parse(endpoint_url).map_err(|_| {
        LakeCatError::InvalidArgument(format!(
            "server endpoint URL must be an absolute http or https URL; {}",
            server_endpoint_url_hash_context(endpoint_url)
        ))
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(LakeCatError::InvalidArgument(format!(
            "server endpoint URL must use http or https scheme; {}",
            server_endpoint_url_hash_context(endpoint_url)
        )));
    }
    if location_has_query_fragment_or_userinfo(endpoint_url) {
        return Err(LakeCatError::InvalidArgument(format!(
            "server endpoint URL must not include query strings, fragments, or userinfo; {}",
            server_endpoint_url_hash_context(endpoint_url)
        )));
    }
    Ok(())
}

fn location_prefix_has_query_fragment_or_userinfo(location_prefix: &str) -> bool {
    location_has_query_fragment_or_userinfo(location_prefix)
}

fn location_has_query_fragment_or_userinfo(location: &str) -> bool {
    Url::parse(location).is_ok_and(|url| {
        url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
    }) || location.contains(['?', '#'])
}

fn location_prefix_has_dot_path_segment(location_prefix: &str) -> bool {
    location_has_dot_path_segment(location_prefix)
}

fn location_has_dot_path_segment(location: &str) -> bool {
    let path = location
        .split_once(['?', '#'])
        .map_or(location, |(path, _)| path);
    path.split('/').any(is_dot_path_segment)
}

fn validate_issuance_mode_provider(
    issuance_mode: CredentialIssuanceMode,
    provider: StorageProvider,
    location_prefix: &str,
) -> LakeCatResult<()> {
    match issuance_mode {
        CredentialIssuanceMode::LocalFileNoSecret if provider != StorageProvider::File => {
            Err(LakeCatError::InvalidArgument(format!(
                "local-file-no-secret issuance mode requires file provider, got '{}'; {}",
                provider.as_str(),
                storage_profile_prefix_hash_context(location_prefix)
            )))
        }
        CredentialIssuanceMode::ShortLivedSecretRef
            if !matches!(
                provider,
                StorageProvider::S3 | StorageProvider::Gcs | StorageProvider::Azure
            ) =>
        {
            Err(LakeCatError::InvalidArgument(format!(
                "short-lived-secret-ref issuance mode requires s3, gcs, or azure provider, got '{}'; {}",
                provider.as_str(),
                storage_profile_prefix_hash_context(location_prefix)
            )))
        }
        _ => Ok(()),
    }
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
            "project id contains unsupported characters; {}",
            project_id_hash_context(project_id)
        )));
    }
    Ok(())
}

fn validate_public_config(config: &BTreeMap<String, String>) -> LakeCatResult<()> {
    for (key, value) in config {
        let normalized = key.to_ascii_lowercase();
        if normalized.contains("secret")
            || normalized.contains("token")
            || normalized.contains("password")
            || normalized.contains("credential")
        {
            return Err(LakeCatError::InvalidArgument(format!(
                "storage profile public config key may expose secret material; {}",
                public_config_key_hash_context(key)
            )));
        }
        if embeds_raw_secret_material(value) {
            return Err(LakeCatError::InvalidArgument(format!(
                "storage profile public config value may expose secret material; {}",
                public_config_key_hash_context(key)
            )));
        }
    }
    Ok(())
}

fn validate_storage_profile_public_config(config: &BTreeMap<String, String>) -> LakeCatResult<()> {
    validate_public_config(config)?;
    for key in config.keys() {
        let normalized = key.to_ascii_lowercase();
        if RESERVED_STORAGE_PROFILE_PUBLIC_CONFIG_KEYS.contains(&normalized.as_str()) {
            return Err(LakeCatError::InvalidArgument(format!(
                "storage profile public config key is reserved for LakeCat credential evidence; {}",
                public_config_key_hash_context(key)
            )));
        }
    }
    Ok(())
}

fn public_config_key_hash_context(key: &str) -> String {
    format!(
        "public-config-key-hash={}",
        content_hash_bytes(key.as_bytes())
    )
}

const RESERVED_STORAGE_PROFILE_PUBLIC_CONFIG_KEYS: &[&str] = &[
    "lakecat.storage-profile-id",
    "lakecat.storage-provider",
    "lakecat.credential-mode",
    "lakecat.governed-read-required",
    "lakecat.authorization-principal",
    "lakecat.max-credential-ttl-seconds",
    "lakecat.credential-kind",
];

fn embeds_raw_secret_material(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
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
        "aws_access_key_id=",
        "aws_secret_access_key=",
        "aws_session_token=",
    ];
    embedded_secret_patterns
        .iter()
        .any(|pattern| normalized.contains(pattern))
}

fn validate_secret_ref(secret_ref: &str) -> LakeCatResult<()> {
    let trimmed = secret_ref.trim();
    if trimmed.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "storage profile secret reference must not be empty".to_string(),
        ));
    }
    let parsed = Url::parse(trimmed).map_err(|_err| {
        LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must be a valid external secret-store URI; {}",
            secret_ref_hash_context(trimmed)
        ))
    })?;
    if !matches!(
        parsed.scheme(),
        "typesec" | "vault" | "aws-sm" | "gcp-sm" | "azure-kv"
    ) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must use an external secret-store URI; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() || !parsed.username().is_empty() {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must not include query strings, fragments, or userinfo; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    if parsed.password().is_some() {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must not include query strings, fragments, or userinfo; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    if secret_ref_has_dot_path_segment(trimmed) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must not include dot path segments; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    if embeds_raw_secret_material(trimmed) {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference must not embed raw secret material; {}",
            secret_ref_hash_context(trimmed)
        )));
    }
    Ok(())
}

fn secret_ref_has_dot_path_segment(secret_ref: &str) -> bool {
    let path = secret_ref
        .split_once(['?', '#'])
        .map_or(secret_ref, |(path, _)| path);
    path.split('/').any(is_dot_path_segment)
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

fn secret_ref_hash_context(secret_ref: &str) -> String {
    format!(
        "secret-ref-hash={}",
        content_hash_bytes(secret_ref.as_bytes())
    )
}

fn storage_profile_prefix_hash_context(location_prefix: &str) -> String {
    format!(
        "storage-profile-prefix-hash={}",
        content_hash_bytes(location_prefix.as_bytes())
    )
}

fn warehouse_storage_root_hash_context(storage_root: &str) -> String {
    format!(
        "warehouse-storage-root-hash={}",
        content_hash_bytes(storage_root.as_bytes())
    )
}

fn server_endpoint_url_hash_context(endpoint_url: &str) -> String {
    format!(
        "server-endpoint-url-hash={}",
        content_hash_bytes(endpoint_url.as_bytes())
    )
}

fn project_id_hash_context(project_id: &str) -> String {
    format!(
        "project-id-hash={}",
        content_hash_bytes(project_id.as_bytes())
    )
}

fn storage_profile_id_hash_context(profile_id: &str) -> String {
    format!(
        "storage-profile-id-hash={}",
        content_hash_bytes(profile_id.as_bytes())
    )
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
    async fn memory_store_rejects_corrupt_server_records_on_read() {
        let store = MemoryCatalogStore::new();
        let record = ServerRecord::new(
            "lakecat-local",
            Some("Local LakeCat".to_string()),
            Some("http://127.0.0.1:8181".to_string()),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_server(record).await.unwrap();

        store
            .state
            .write()
            .await
            .servers
            .get_mut("lakecat-local")
            .unwrap()
            .endpoint_url = Some("http://127.0.0.1:8181?token=secret".to_string());

        let err = store.list_servers().await.unwrap_err();
        let message = err.to_string();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("server endpoint URL")
                    || message.contains("server-endpoint-url-hash=sha256:")
        ));
        assert!(!message.contains("token=secret"));
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
    async fn memory_store_rejects_corrupt_warehouse_records_on_read() {
        let store = MemoryCatalogStore::new();
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
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_warehouse(record).await.unwrap();

        store
            .state
            .write()
            .await
            .warehouses
            .get_mut("local")
            .unwrap()
            .storage_root = Some("file:///tmp/lakecat?token=secret".to_string());

        let err = store.load_warehouse(&warehouse).await.unwrap_err();
        let message = err.to_string();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("warehouse storage root")
                    || message.contains("warehouse-storage-root-hash=sha256:")
        ));
        assert!(!message.contains("token=secret"));

        let err = store.list_warehouses().await.unwrap_err();
        assert!(
            err.to_string()
                .contains("warehouse-storage-root-hash=sha256:")
        );
    }

    #[tokio::test]
    async fn memory_store_rejects_corrupt_storage_profiles_on_read() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let table = TableRecord::new(
            TableIdent::new(
                warehouse.clone(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            ),
            "s3://lakecat-demo/events/table".to_string(),
            None,
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );
        let profile = StorageProfile::new(
            "s3-events",
            warehouse.clone(),
            "s3://lakecat-demo/events",
            StorageProvider::S3,
            CredentialIssuanceMode::ShortLivedSecretRef,
            Some("typesec://lakecat/local/s3-events".to_string()),
            BTreeMap::new(),
        )
        .unwrap();
        store.upsert_storage_profile(profile).await.unwrap();

        let key = storage_profile_key(&warehouse, "s3-events");
        store
            .state
            .write()
            .await
            .storage_profiles
            .get_mut(&key)
            .unwrap()
            .profile_id = "s3-events?token=secret".to_string();

        let err = store.list_storage_profiles(&warehouse).await.unwrap_err();
        let message = err.to_string();
        assert!(message.contains("storage-profile-id-hash=sha256:"));
        assert!(!message.contains("token=secret"));

        let err = store.storage_profile_for_table(&table).await.unwrap_err();
        let message = err.to_string();
        assert!(message.contains("storage-profile-id-hash=sha256:"));
        assert!(!message.contains("token=secret"));
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
    async fn memory_store_rejects_corrupt_project_records_on_read() {
        let store = MemoryCatalogStore::new();
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
        let project = ProjectRecord::new(
            "default",
            Some("lakecat-local".to_string()),
            Some("QueryGraph Project".to_string()),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_project(project).await.unwrap();

        store
            .state
            .write()
            .await
            .projects
            .get_mut("default")
            .unwrap()
            .server_id = Some("lakecat-local?token=secret".to_string());

        let err = store.list_projects().await.unwrap_err();
        let message = err.to_string();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("project") || message.contains("identifier")
        ));
        assert!(!message.contains("token=secret"));
    }

    #[tokio::test]
    async fn memory_store_loads_and_drops_namespaces() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let empty_namespace = "empty".parse::<Namespace>().unwrap();

        assert!(matches!(
            store.load_namespace(&warehouse, &empty_namespace).await,
            Err(LakeCatError::NotFound { object, name })
                if object == "namespace" && name == "empty"
        ));

        store
            .create_namespace(&warehouse, empty_namespace.clone())
            .await
            .unwrap();
        assert_eq!(
            store
                .load_namespace(&warehouse, &empty_namespace)
                .await
                .unwrap(),
            empty_namespace.clone()
        );
        assert_eq!(
            store
                .drop_namespace(&warehouse, &empty_namespace)
                .await
                .unwrap(),
            empty_namespace
        );
        assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);

        let table_namespace = "has_table".parse::<Namespace>().unwrap();
        let table = TableRecord::new(
            TableIdent::new(
                warehouse.clone(),
                table_namespace.clone(),
                TableName::new("events").unwrap(),
            ),
            "file:///tmp/has_table".to_string(),
            Some("file:///tmp/has_table/metadata/00000.json".to_string()),
            serde_json::json!({"format-version": 3}),
            Principal::anonymous(),
        );
        store.create_table(table).await.unwrap();
        assert!(matches!(
            store.drop_namespace(&warehouse, &table_namespace).await,
            Err(LakeCatError::Conflict(message)) if message.contains("tables")
        ));

        let view_namespace = "has_view".parse::<Namespace>().unwrap();
        store
            .create_namespace(&warehouse, view_namespace.clone())
            .await
            .unwrap();
        store
            .upsert_view(
                ViewRecord::new(
                    warehouse.clone(),
                    view_namespace.clone(),
                    TableName::new("active_customers").unwrap(),
                    "select * from customers",
                    "duckdb",
                    None,
                    BTreeMap::new(),
                    Principal::anonymous(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert!(matches!(
            store.drop_namespace(&warehouse, &view_namespace).await,
            Err(LakeCatError::Conflict(message)) if message.contains("views")
        ));

        let policy_namespace = "has_policy".parse::<Namespace>().unwrap();
        store
            .create_namespace(&warehouse, policy_namespace.clone())
            .await
            .unwrap();
        store
            .upsert_policy_binding(
                PolicyBinding::new(
                    "namespace-policy",
                    warehouse.clone(),
                    Some(policy_namespace.clone()),
                    None,
                    true,
                    serde_json::json!({"permission": []}),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert!(matches!(
            store.drop_namespace(&warehouse, &policy_namespace).await,
            Err(LakeCatError::Conflict(message)) if message.contains("policy bindings")
        ));
    }

    #[tokio::test]
    async fn memory_store_rejects_deserialized_invalid_policy_bindings() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let binding = PolicyBinding {
            policy_id: "table-policy".to_string(),
            warehouse: warehouse.clone(),
            namespace: None,
            table: Some(TableName::new("events").unwrap()),
            enforced: true,
            odrl: serde_json::json!({"uid": "policy:table-policy"}),
            updated_at: Utc::now(),
        };

        let err = store.upsert_policy_binding(binding).await.unwrap_err();

        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table-scoped policy binding requires namespace")
        ));
        assert_eq!(
            store.list_policy_bindings(&warehouse).await.unwrap(),
            vec![]
        );
    }

    #[tokio::test]
    async fn memory_store_rejects_corrupt_policy_bindings_on_read() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let table = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").unwrap(),
        );
        let binding = PolicyBinding::new(
            "table-policy",
            warehouse.clone(),
            Some(namespace),
            Some(TableName::new("events").unwrap()),
            true,
            serde_json::json!({"uid": "policy:table-policy"}),
        )
        .unwrap();
        store.upsert_policy_binding(binding).await.unwrap();

        let key = policy_binding_key(&warehouse, "table-policy");
        store
            .state
            .write()
            .await
            .policy_bindings
            .get_mut(&key)
            .unwrap()
            .namespace = None;

        let err = store.list_policy_bindings(&warehouse).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table-scoped policy binding requires namespace")
        ));

        let err = store.policy_bindings_for_table(&table).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table-scoped policy binding requires namespace")
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
        let view = store.upsert_view(view).await.unwrap();
        assert_eq!(view.view_version, 1);

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
        .unwrap()
        .with_columns(vec![ViewColumnRecord {
            name: "id".to_string(),
            data_type: serde_json::json!("long"),
            nullable: false,
            comment: Some("Customer identifier".to_string()),
        }])
        .unwrap();
        let updated = store
            .upsert_view_if_version(updated, Some(1))
            .await
            .unwrap();
        assert_eq!(updated.view_version, 2);
        let stale = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("active_customers").unwrap(),
            "select id from customers where active",
            "sql",
            Some(3),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        let err = store
            .upsert_view_if_version(stale, Some(1))
            .await
            .expect_err("stale expected view version must conflict");
        assert!(matches!(
            err,
            LakeCatError::Conflict(message) if message.contains("expected version 1")
        ));
        let receipts = store
            .list_view_version_receipts(
                &warehouse,
                &namespace,
                &TableName::new("active_customers").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(receipts.len(), 2);
        assert_eq!(
            receipts[0].stable_id,
            "lakecat:view:local:default:active_customers"
        );
        assert_eq!(receipts[0].view_version, 1);
        assert_eq!(receipts[0].previous_view_version, None);
        assert_eq!(receipts[0].previous_receipt_hash, None);
        assert_eq!(receipts[0].operation, ViewVersionOperation::Upsert);
        assert!(!receipts[0].view_hash.is_empty());
        let first_receipt_hash = view_receipt_hash(&receipts[0]).unwrap();
        assert_eq!(receipts[1].view_version, 2);
        assert_eq!(receipts[1].previous_view_version, Some(1));
        assert_eq!(
            receipts[1].previous_receipt_hash.as_deref(),
            Some(first_receipt_hash.as_str())
        );
        assert_ne!(receipts[0].view_hash, receipts[1].view_hash);
        let second_receipt_hash = view_receipt_hash(&receipts[1]).unwrap();

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
        let err = store
            .drop_view_if_version(
                &warehouse,
                &namespace,
                &TableName::new("active_customers").unwrap(),
                Principal::anonymous(),
                Some(1),
            )
            .await
            .expect_err("stale expected view version must not drop the view");
        assert!(matches!(
            err,
            LakeCatError::Conflict(message) if message.contains("expected version 1")
        ));
        assert_eq!(
            store
                .list_view_version_receipts(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap(),
                )
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            store
                .drop_view_if_version(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap(),
                    Principal::anonymous(),
                    Some(2)
                )
                .await
                .unwrap(),
            updated
        );
        let receipts = store
            .list_view_version_receipts(
                &warehouse,
                &namespace,
                &TableName::new("active_customers").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(receipts.len(), 3);
        assert_eq!(receipts[2].stable_id, receipts[1].stable_id);
        assert_eq!(receipts[2].view_version, 2);
        assert_eq!(receipts[2].previous_view_version, Some(2));
        assert_eq!(
            receipts[2].previous_receipt_hash.as_deref(),
            Some(second_receipt_hash.as_str())
        );
        assert_eq!(receipts[2].operation, ViewVersionOperation::Drop);
        assert_eq!(receipts[2].view_hash, receipts[1].view_hash);
        let namespace_receipts = store
            .list_namespace_view_version_receipts(&warehouse, &namespace)
            .await
            .unwrap();
        assert_eq!(namespace_receipts, receipts);
        assert_eq!(
            store.list_views(&warehouse, &namespace).await.unwrap(),
            Vec::<ViewRecord>::new()
        );
        assert!(matches!(
            store
                .drop_view(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap(),
                    Principal::anonymous()
                )
                .await,
            Err(LakeCatError::NotFound { object, name })
                if object == "view" && name == "active_customers"
        ));
        let recreated = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("active_customers").unwrap(),
            "select id from customers where active",
            "sql",
            Some(3),
            BTreeMap::from([("owner".to_string(), "lakecat".to_string())]),
            Principal::anonymous(),
        )
        .unwrap();
        let recreated = store.upsert_view(recreated).await.unwrap();
        assert_eq!(recreated.view_version, 3);
        let receipts = store
            .list_view_version_receipts(
                &warehouse,
                &namespace,
                &TableName::new("active_customers").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(receipts.len(), 4);
        let drop_receipt_hash = view_receipt_hash(&receipts[2]).unwrap();
        assert_eq!(receipts[3].stable_id, receipts[2].stable_id);
        assert_eq!(receipts[3].view_version, 3);
        assert_eq!(receipts[3].previous_view_version, Some(2));
        assert_eq!(
            receipts[3].previous_receipt_hash.as_deref(),
            Some(drop_receipt_hash.as_str())
        );
        assert_eq!(receipts[3].operation, ViewVersionOperation::Upsert);
        assert_ne!(receipts[3].view_hash, receipts[2].view_hash);
        assert_eq!(
            store
                .load_view(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap()
                )
                .await
                .unwrap(),
            recreated
        );
    }

    #[tokio::test]
    async fn memory_store_rejects_corrupt_view_records_on_read() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let view_name = TableName::new("active_customers").unwrap();
        let view = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            view_name.clone(),
            "select * from customers where active",
            "sql",
            Some(1),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        let view = store.upsert_view(view).await.unwrap();

        store
            .state
            .write()
            .await
            .views
            .get_mut(&view_key(&view))
            .unwrap()
            .sql = "   ".to_string();

        let err = store
            .load_view(&warehouse, &namespace, &view_name)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("view SQL must not be empty")
        ));

        let err = store.list_views(&warehouse, &namespace).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("view SQL must not be empty")
        ));

        let err = store
            .drop_view(&warehouse, &namespace, &view_name, Principal::anonymous())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("view SQL must not be empty")
        ));
    }

    #[tokio::test]
    async fn memory_store_rejects_corrupt_view_receipts_on_read() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let view_name = TableName::new("active_customers").unwrap();
        let view = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            view_name.clone(),
            "select * from customers where active",
            "sql",
            Some(1),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_view(view).await.unwrap();

        store
            .state
            .write()
            .await
            .view_version_receipts
            .first_mut()
            .unwrap()
            .view_hash = "sha256:short".to_string();

        let err = store
            .list_view_version_receipts(&warehouse, &namespace, &view_name)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("view receipt hash must be a SHA-256 digest")
        ));

        let err = store
            .list_namespace_view_version_receipts(&warehouse, &namespace)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("view receipt hash must be a SHA-256 digest")
        ));
    }

    #[tokio::test]
    async fn memory_store_rejects_corrupt_view_receipt_chain_links_on_read() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let view_name = TableName::new("active_customers").unwrap();
        let view = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            view_name.clone(),
            "select * from customers where active",
            "sql",
            Some(1),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_view(view).await.unwrap();
        let updated = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            view_name.clone(),
            "select id from customers where active",
            "sql",
            Some(2),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        store
            .upsert_view_if_version(updated, Some(1))
            .await
            .unwrap();

        let forged_hash = content_hash_json(&serde_json::json!({"forged": "previous"})).unwrap();
        store
            .state
            .write()
            .await
            .view_version_receipts
            .get_mut(1)
            .unwrap()
            .previous_receipt_hash = Some(forged_hash);

        let err = store
            .list_view_version_receipts(&warehouse, &namespace, &view_name)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("view receipt chain previous links must match")
        ));

        let err = store
            .list_namespace_view_version_receipts(&warehouse, &namespace)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("view receipt chain previous links must match")
        ));
    }

    #[tokio::test]
    async fn memory_store_rejects_corrupt_view_receipt_chain_before_mutation() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let view_name = TableName::new("active_customers").unwrap();
        let view = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            view_name.clone(),
            "select * from customers where active",
            "sql",
            Some(1),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        store.upsert_view(view).await.unwrap();
        let updated = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            view_name.clone(),
            "select id from customers where active",
            "sql",
            Some(2),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        store
            .upsert_view_if_version(updated, Some(1))
            .await
            .unwrap();

        let forged_hash = content_hash_json(&serde_json::json!({"forged": "previous"})).unwrap();
        store
            .state
            .write()
            .await
            .view_version_receipts
            .get_mut(1)
            .unwrap()
            .previous_receipt_hash = Some(forged_hash);

        let attempted = ViewRecord::new(
            warehouse.clone(),
            namespace.clone(),
            view_name.clone(),
            "select id, email from customers where active",
            "sql",
            Some(3),
            BTreeMap::new(),
            Principal::anonymous(),
        )
        .unwrap();
        let err = store
            .upsert_view_if_version(attempted, Some(2))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("view receipt chain previous links must match")
        ));

        let state = store.state.read().await;
        let active = state
            .views
            .get(&view_key_parts(&warehouse, &namespace, &view_name))
            .unwrap();
        assert_eq!(active.view_version, 2);
        assert_eq!(state.view_version_receipts.len(), 2);
    }

    #[tokio::test]
    async fn memory_store_rejects_corrupt_soft_delete_records_on_restore() {
        let store = MemoryCatalogStore::new();
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

        let key = table_key(&ident);
        store
            .state
            .write()
            .await
            .soft_deletes
            .get_mut(&key)
            .unwrap()
            .version += 1;

        let err = store
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
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("soft-delete version does not match table record")
        ));
        assert!(store.state.read().await.soft_deletes.contains_key(&key));
        assert!(matches!(
            store.load_table(&ident).await,
            Err(LakeCatError::NotFound { .. })
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

    #[tokio::test]
    async fn memory_store_omits_table_from_unscoped_audit_outbox_events() {
        let store = MemoryCatalogStore::new();
        store
            .record_audit_event(
                CatalogAuditEvent::new(
                    "catalog.config-read",
                    None,
                    Principal::anonymous(),
                    serde_json::json!({
                        "event-type": "catalog.config-read",
                        "authorization-receipt": {
                            "engine": "typesec",
                            "allowed": true,
                            "action": "catalog-config"
                        },
                        "warehouse": "local"
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
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
                        "version": 1
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
        let config = pending
            .iter()
            .find(|event| event.event_type == "catalog.config-read")
            .expect("config-read event");
        assert!(
            config.payload.get("table").is_none(),
            "unscoped config-read wrapper must not carry table evidence"
        );
        let table = pending
            .iter()
            .find(|event| event.event_type == "table.loaded")
            .expect("table-loaded event");
        assert!(
            table.payload.get("table").is_some(),
            "table-scoped wrapper must preserve table evidence"
        );
    }

    #[tokio::test]
    async fn memory_store_duplicate_audit_write_does_not_duplicate_outbox() {
        let store = MemoryCatalogStore::new();
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let mut event = CatalogAuditEvent::new(
            "querygraph.bootstrap",
            Some(ident.clone()),
            Principal::anonymous(),
            serde_json::json!({
                "event-type": "querygraph.bootstrap",
                "table": ident,
                "manifest-hash": "lakecat:test"
            }),
        )
        .unwrap();
        event.created_at = "2026-01-01T00:00:00Z".parse().unwrap();

        store.record_audit_event(event.clone()).await.unwrap();
        let err = store.record_audit_event(event).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("duplicate audit event id would duplicate outbox replay evidence")
        ));
        let state = store.state.read().await;
        assert_eq!(state.audit_events.len(), 1);
        assert_eq!(state.outbox_events.len(), 1);
    }

    #[tokio::test]
    async fn memory_store_rejects_malformed_outbox_delivery_ids() {
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
                        "manifest-hash": "lakecat:test"
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let err = store
            .mark_outbox_delivered(&["sha256:short".to_string()])
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("outbox event id must be full SHA-256 evidence")
        ));
        let pending = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].delivered_at.is_none());
    }

    #[tokio::test]
    async fn memory_store_rejects_corrupt_pending_outbox_event_ids() {
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
                        "manifest-hash": "lakecat:test"
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        store.state.write().await.outbox_events[0].event_id = "sha256:short".to_string();

        let err = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("pending outbox event id does not match payload hash")
                    && message.contains("event-id-hash=sha256:")
                    && message.contains("event-type-hash=sha256:")
                    && message.contains("payload-hash=sha256:")
            && !message.contains("sha256:short")
        ));
    }

    #[tokio::test]
    async fn memory_store_rejects_blank_pending_outbox_event_types() {
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
                        "manifest-hash": "lakecat:test"
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let blank_payload = serde_json::json!({
            "event-type": " ",
            "manifest-hash": "lakecat:test"
        });
        let blank_payload_hash = content_hash_json(&blank_payload).unwrap();
        {
            let mut state = store.state.write().await;
            state.outbox_events[0].event_id = blank_payload_hash;
            state.outbox_events[0].event_type = " ".to_string();
            state.outbox_events[0].payload = blank_payload;
        }

        let err = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("pending outbox event type must not be empty")
                    && message.contains("event-id-hash=sha256:")
                    && message.contains("event-type-hash=sha256:")
                    && message.contains("payload-hash=sha256:")
                    && !message.contains("manifest-hash")
                    && !message.contains("lakecat:test")
        ));
    }

    #[tokio::test]
    async fn memory_store_rejects_blank_pending_outbox_payload_event_types() {
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
                        "manifest-hash": "lakecat:test"
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let blank_payload = serde_json::json!({
            "event-type": " ",
            "manifest-hash": "lakecat:test"
        });
        let blank_payload_hash = content_hash_json(&blank_payload).unwrap();
        {
            let mut state = store.state.write().await;
            state.outbox_events[0].event_id = blank_payload_hash;
            state.outbox_events[0].payload = blank_payload;
        }

        let err = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("pending outbox payload event-type must not be empty")
                    && message.contains("event-id-hash=sha256:")
                    && message.contains("event-type-hash=sha256:")
                    && message.contains("payload-event-type-hash=sha256:")
                    && message.contains("payload-hash=sha256:")
                    && !message.contains("manifest-hash")
                    && !message.contains("lakecat:test")
        ));
    }

    #[tokio::test]
    async fn memory_store_rejects_blank_pending_outbox_sinks() {
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
                        "manifest-hash": "lakecat:test"
                    }),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        store.state.write().await.outbox_events[0].sink = " ".to_string();

        let err = store.pending_outbox_events(None, 10).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains("pending outbox event sink must not be empty")
                    && message.contains("event-id-hash=sha256:")
                    && message.contains("event-type-hash=sha256:")
                    && message.contains("payload-hash=sha256:")
                    && !message.contains("manifest-hash")
                    && !message.contains("lakecat:test")
        ));
    }

    #[tokio::test]
    async fn memory_store_rejects_audit_event_type_drift_before_outbox() {
        let store = MemoryCatalogStore::new();
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let mut event = CatalogAuditEvent::new(
            "querygraph.bootstrap",
            Some(ident.clone()),
            Principal::anonymous(),
            serde_json::json!({
                "event-type": "querygraph.bootstrap",
                "table": ident,
                "manifest-hash": "lakecat:test"
            }),
        )
        .unwrap();
        event.event_type = "querygraph.bootstrap.drifted".to_string();

        let err = store.record_audit_event(event).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("audit event type does not match payload")
        ));
        let state = store.state.read().await;
        assert!(state.audit_events.is_empty());
        assert!(state.outbox_events.is_empty());
    }

    #[tokio::test]
    async fn memory_store_rejects_audit_events_without_request_hash() {
        let store = MemoryCatalogStore::new();
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let mut event = CatalogAuditEvent::new(
            "querygraph.bootstrap",
            Some(ident.clone()),
            Principal::anonymous(),
            serde_json::json!({
                "event-type": "querygraph.bootstrap",
                "table": ident,
                "manifest-hash": "lakecat:test"
            }),
        )
        .unwrap();
        event.request_hash = None;

        let err = store.record_audit_event(event).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("audit event request hash is required")
        ));
        let state = store.state.read().await;
        assert!(state.audit_events.is_empty());
        assert!(state.outbox_events.is_empty());
    }

    #[tokio::test]
    async fn memory_store_rejects_audit_payload_table_scope_drift() {
        let store = MemoryCatalogStore::new();
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let other_ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("other_events").unwrap(),
        );
        let event = CatalogAuditEvent::new(
            "querygraph.bootstrap",
            Some(ident),
            Principal::anonymous(),
            serde_json::json!({
                "event-type": "querygraph.bootstrap",
                "table": other_ident,
                "manifest-hash": "lakecat:test"
            }),
        )
        .unwrap();

        let err = store.record_audit_event(event).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("audit event payload table scope does not match")
        ));
        let state = store.state.read().await;
        assert!(state.audit_events.is_empty());
        assert!(state.outbox_events.is_empty());
    }

    #[tokio::test]
    async fn memory_store_rejects_bare_table_name_audit_payload_scope() {
        let store = MemoryCatalogStore::new();
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let event = CatalogAuditEvent::new(
            "table.commits-listed",
            Some(ident),
            Principal::anonymous(),
            serde_json::json!({
                "event-type": "table.commits-listed",
                "table": "events",
                "commit-count": 0
            }),
        )
        .unwrap();

        let err = store.record_audit_event(event).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("audit event payload missing warehouse scope for table")
        ));
        let state = store.state.read().await;
        assert!(state.audit_events.is_empty());
        assert!(state.outbox_events.is_empty());
    }

    #[tokio::test]
    async fn memory_store_rejects_audit_authorization_principal_drift() {
        let store = MemoryCatalogStore::new();
        let event_principal =
            Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent).unwrap();
        let receipt_principal =
            Principal::new("human:operator", lakecat_core::PrincipalKind::Human).unwrap();
        let event = CatalogAuditEvent::new(
            "querygraph.bootstrap",
            None,
            event_principal,
            serde_json::json!({
                "event-type": "querygraph.bootstrap",
                "authorization-receipt": {
                    "engine": "typesec",
                    "allowed": true,
                    "principal": receipt_principal,
                    "action": "querygraph.bootstrap"
                },
                "manifest-hash": "lakecat:test"
            }),
        )
        .unwrap();

        let err = store.record_audit_event(event).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains(
                    "audit event authorization receipt principal does not match event principal"
                )
        ));
        let state = store.state.read().await;
        assert!(state.audit_events.is_empty());
        assert!(state.outbox_events.is_empty());
    }

    #[tokio::test]
    async fn memory_store_rejects_audit_authorization_receipts_without_action() {
        let store = MemoryCatalogStore::new();
        let principal =
            Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent).unwrap();
        let event = CatalogAuditEvent::new(
            "querygraph.bootstrap",
            None,
            principal.clone(),
            serde_json::json!({
                "event-type": "querygraph.bootstrap",
                "authorization-receipt": {
                    "engine": "typesec",
                    "allowed": true,
                    "principal": principal
                },
                "manifest-hash": "lakecat:test"
            }),
        )
        .unwrap();

        let err = store.record_audit_event(event).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("audit event authorization receipt action is required")
        ));
        let state = store.state.read().await;
        assert!(state.audit_events.is_empty());
        assert!(state.outbox_events.is_empty());
    }

    #[tokio::test]
    async fn memory_store_orders_pending_outbox_events_deterministically() {
        let store = MemoryCatalogStore::new();
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let mut events = Vec::new();
        for event_type in ["querygraph.bootstrap.b", "querygraph.bootstrap.a"] {
            let mut event = CatalogAuditEvent::new(
                event_type,
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": event_type,
                    "table": ident.clone(),
                    "sequence": event_type,
                }),
            )
            .unwrap();
            event.created_at = "2026-01-01T00:00:00Z".parse().unwrap();
            let audit_event_id = audit_event_id(&event).unwrap();
            let outbox_payload = audit_outbox_payload(&audit_event_id, &event);
            let outbox_event = outbox_event_from_payload(&outbox_payload, event.created_at)
                .expect("test event should produce an outbox event");
            events.push((outbox_event.event_id, event));
        }
        events.sort_by(|left, right| right.0.cmp(&left.0));
        for (_, event) in events {
            store.record_audit_event(event).await.unwrap();
        }

        let pending = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
            .await
            .unwrap();
        let event_ids = pending
            .iter()
            .map(|event| event.event_id.clone())
            .collect::<Vec<_>>();
        let mut sorted_event_ids = event_ids.clone();
        sorted_event_ids.sort();
        assert_eq!(event_ids, sorted_event_ids);
        assert_eq!(
            store
                .mark_outbox_delivered(&[event_ids[0].clone(), event_ids[0].clone()])
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn memory_store_limits_pending_outbox_after_deterministic_ordering() {
        let store = MemoryCatalogStore::new();
        let ident = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let mut expected = Vec::new();
        let mut events = Vec::new();
        for (event_type, created_at) in [
            ("querygraph.bootstrap.late", "2026-01-01T00:00:02Z"),
            ("querygraph.bootstrap.tie-b", "2026-01-01T00:00:01Z"),
            ("querygraph.bootstrap.tie-a", "2026-01-01T00:00:01Z"),
        ] {
            let mut event = CatalogAuditEvent::new(
                event_type,
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": event_type,
                    "table": ident.clone(),
                    "sequence": event_type,
                }),
            )
            .unwrap();
            event.created_at = created_at.parse().unwrap();
            let audit_event_id = audit_event_id(&event).unwrap();
            let outbox_payload = audit_outbox_payload(&audit_event_id, &event);
            let outbox_event = outbox_event_from_payload(&outbox_payload, event.created_at)
                .expect("test event should produce an outbox event");
            expected.push((outbox_event.created_at, outbox_event.event_id.clone()));
            events.push((outbox_event.event_id, event));
        }
        events.sort_by(|left, right| right.0.cmp(&left.0));
        for (_, event) in events {
            store.record_audit_event(event).await.unwrap();
        }
        expected.sort();

        let pending = store
            .pending_outbox_events(Some("lakecat.lineage-and-graph"), 2)
            .await
            .unwrap();
        let event_ids = pending
            .iter()
            .map(|event| event.event_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            event_ids,
            expected
                .iter()
                .take(2)
                .map(|(_, event_id)| event_id.clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn memory_store_rejects_deserialized_empty_table_locations() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").unwrap(),
        );
        let table = TableRecord {
            ident: ident.clone(),
            location: "   ".to_string(),
            metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            metadata: serde_json::json!({"format-version": 3}),
            created: AuditStamp::now(Principal::anonymous()),
            updated_at: Utc::now(),
            version: 0,
        };

        let err = store.create_table(table).await.unwrap_err();

        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table location must not be empty")
        ));
        assert!(matches!(
            store.load_table(&ident).await,
            Err(LakeCatError::NotFound { object, name })
                if object == "table" && name == ident.stable_id()
        ));
        assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);
    }

    #[tokio::test]
    async fn memory_store_rejects_deserialized_invalid_table_metadata() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").unwrap(),
        );
        let base = TableRecord {
            ident: ident.clone(),
            location: "file:///tmp/events".to_string(),
            metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            metadata: serde_json::json!({"format-version": 3}),
            created: AuditStamp::now(Principal::anonymous()),
            updated_at: Utc::now(),
            version: 0,
        };

        let mut empty_metadata_location = base.clone();
        empty_metadata_location.metadata_location = Some("  ".to_string());
        let err = store
            .create_table(empty_metadata_location)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table metadata location must not be empty")
        ));

        let mut non_object_metadata = base;
        non_object_metadata.metadata = serde_json::json!(["not", "metadata"]);
        let err = store.create_table(non_object_metadata).await.unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table metadata must be a JSON object")
        ));

        let mut missing_format_version = TableRecord {
            ident: ident.clone(),
            location: "file:///tmp/events".to_string(),
            metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            metadata: serde_json::json!({"current-snapshot-id": 42}),
            created: AuditStamp::now(Principal::anonymous()),
            updated_at: Utc::now(),
            version: 0,
        };
        let err = store
            .create_table(missing_format_version.clone())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table metadata format-version must be present")
        ));

        missing_format_version.metadata = serde_json::json!({"format-version": 0});
        let err = store
            .create_table(missing_format_version)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table metadata format-version must be positive")
        ));

        assert!(matches!(
            store.load_table(&ident).await,
            Err(LakeCatError::NotFound { object, name })
                if object == "table" && name == ident.stable_id()
        ));
        assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);
    }

    #[tokio::test]
    async fn memory_store_rejects_deserialized_invalid_table_commits() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").unwrap(),
        );
        store
            .create_namespace(&warehouse, namespace.clone())
            .await
            .unwrap();
        store
            .create_table(TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            ))
            .await
            .unwrap();

        let base_commit = TableCommit {
            requirements: vec![],
            updates: vec![serde_json::json!({"action": "noop"})],
            expected_previous_metadata_location: Some(
                "file:///tmp/events/metadata/00000.json".to_string(),
            ),
            new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
            new_metadata: Some(serde_json::json!({"format-version": 3})),
            idempotency_key: None,
            idempotency_request_hash: None,
            principal: Principal::anonymous(),
            authorization_receipt: None,
        };

        let mut blank_idempotency_key = base_commit.clone();
        blank_idempotency_key.idempotency_key = Some("  ".to_string());
        let err = store
            .commit_table(&ident, blank_idempotency_key)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table commit idempotency key may only contain")
        ));

        let mut request_hash_without_key = base_commit.clone();
        request_hash_without_key.idempotency_request_hash =
            Some(content_hash_bytes("commit-request".as_bytes()));
        let err = store
            .commit_table(&ident, request_hash_without_key)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains(
                    "table commit idempotency request hash requires an idempotency key"
                )
        ));

        let mut malformed_request_hash = base_commit.clone();
        malformed_request_hash.idempotency_key = Some("commit-1".to_string());
        malformed_request_hash.idempotency_request_hash = Some("sha256:short".to_string());
        let err = store
            .commit_table(&ident, malformed_request_hash)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains(
                    "table commit idempotency request hash must be full SHA-256 evidence"
                )
        ));

        let err = store
            .replay_table_commit(
                &ident,
                " ",
                &content_hash_bytes("commit-request".as_bytes()),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("table commit idempotency key may only contain")
        ));

        let err = store
            .replay_table_commit(&ident, "commit-1", "sha256:short")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains(
                    "table commit idempotency request hash must be full SHA-256 evidence"
                )
        ));
        {
            let table = store.load_table(&ident).await.unwrap();
            assert_eq!(table.version, 0);
            assert_eq!(
                table.metadata_location.as_deref(),
                Some("file:///tmp/events/metadata/00000.json")
            );
            let state = store.state.read().await;
            assert!(
                state.commits.is_empty(),
                "invalid idempotency evidence must fail before pointer-log insertion"
            );
            assert!(
                state.audit_events.is_empty(),
                "invalid idempotency evidence must fail before audit insertion"
            );
            assert!(
                state.outbox_events.is_empty(),
                "invalid idempotency evidence must fail before outbox insertion"
            );
            assert!(
                state.idempotency.is_empty(),
                "invalid idempotency evidence must fail before idempotency replay state"
            );
        }

        let mut empty_new_location = base_commit.clone();
        empty_new_location.new_metadata_location = Some("  ".to_string());
        let err = store
            .commit_table(&ident, empty_new_location)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("new table metadata location must not be empty")
        ));

        let mut non_object_metadata = base_commit;
        non_object_metadata.new_metadata = Some(serde_json::json!("not metadata"));
        let err = store
            .commit_table(&ident, non_object_metadata)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("new table metadata must be a JSON object")
        ));

        let missing_format_version = TableCommit {
            requirements: vec![],
            updates: vec![serde_json::json!({"action": "noop"})],
            expected_previous_metadata_location: Some(
                "file:///tmp/events/metadata/00000.json".to_string(),
            ),
            new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
            new_metadata: Some(serde_json::json!({"current-snapshot-id": 42})),
            idempotency_key: None,
            idempotency_request_hash: None,
            principal: Principal::anonymous(),
            authorization_receipt: None,
        };
        let err = store
            .commit_table(&ident, missing_format_version)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("new table metadata format-version must be present")
        ));

        let zero_format_version = TableCommit {
            requirements: vec![],
            updates: vec![serde_json::json!({"action": "noop"})],
            expected_previous_metadata_location: Some(
                "file:///tmp/events/metadata/00000.json".to_string(),
            ),
            new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
            new_metadata: Some(serde_json::json!({"format-version": 0})),
            idempotency_key: None,
            idempotency_request_hash: None,
            principal: Principal::anonymous(),
            authorization_receipt: None,
        };
        let err = store
            .commit_table(&ident, zero_format_version)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::InvalidArgument(message)
                if message.contains("new table metadata format-version must be positive")
        ));

        let table = store.load_table(&ident).await.unwrap();
        assert_eq!(table.version, 0);
        assert_eq!(
            table.metadata_location.as_deref(),
            Some("file:///tmp/events/metadata/00000.json")
        );
        assert_eq!(
            store.table_commit_records(&ident, 0, None).await.unwrap(),
            vec![]
        );
        assert_eq!(store.pending_outbox_events(None, 10).await.unwrap(), vec![]);
    }

    #[tokio::test]
    async fn memory_store_commit_records_table_commit_outbox_event() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").unwrap(),
        );
        store
            .create_namespace(&warehouse, namespace.clone())
            .await
            .unwrap();
        store
            .create_table(TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            ))
            .await
            .unwrap();

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
        let replayed = store.commit_table(&ident, commit).await.unwrap();
        assert_eq!(replayed.version, 1);

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
        assert_eq!(
            pending[0].payload["commit"]["snapshot_id"],
            serde_json::json!(0)
        );
        assert_eq!(
            pending[0].payload["authorization-receipt"]["engine"],
            serde_json::json!("typesec")
        );
        assert!(!pending[0].payload.to_string().contains("commit-1"));
    }

    #[tokio::test]
    async fn memory_store_stale_pointer_conflict_uses_location_hashes() {
        let store = MemoryCatalogStore::new();
        let ident = table_ident("local", "default", "events").unwrap();
        store
            .create_table(TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            ))
            .await
            .unwrap();

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
        let message = err.to_string();
        assert!(message.contains("expected-metadata-location-hash=sha256:"));
        assert!(message.contains("actual-metadata-location-hash=sha256:"));
        assert!(!message.contains("stale.json"));
        assert!(!message.contains("00000.json"));
    }

    #[tokio::test]
    async fn memory_store_rejects_malformed_commit_history_records() {
        let store = MemoryCatalogStore::new();
        let warehouse = WarehouseName::new("local").unwrap();
        let namespace = "default".parse::<Namespace>().unwrap();
        let ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").unwrap(),
        );
        store
            .create_namespace(&warehouse, namespace.clone())
            .await
            .unwrap();
        store
            .create_table(TableRecord::new(
                ident.clone(),
                "file:///tmp/events".to_string(),
                Some("file:///tmp/events/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            ))
            .await
            .unwrap();
        store
            .commit_table(
                &ident,
                TableCommit {
                    requirements: vec![],
                    updates: vec![serde_json::json!({"action": "noop"})],
                    expected_previous_metadata_location: Some(
                        "file:///tmp/events/metadata/00000.json".to_string(),
                    ),
                    new_metadata_location: Some(
                        "file:///tmp/events/metadata/00001.json".to_string(),
                    ),
                    new_metadata: Some(serde_json::json!({"format-version": 3})),
                    idempotency_key: Some("commit-1".to_string()),
                    idempotency_request_hash: None,
                    principal: Principal::anonymous(),
                    authorization_receipt: None,
                },
            )
            .await
            .unwrap();

        store.state.write().await.commits[0].response_hash = "sha256:short".to_string();

        let err = store
            .table_commit_records(&ident, 0, None)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains(
                    "table commit record response hash must be full SHA-256 evidence"
                )
        ));

        store.state.write().await.commits[0].response_hash =
            content_hash_bytes("response".as_bytes());
        store.state.write().await.commits[0].policy_hash = Some("sha256:short".to_string());

        let err = store
            .table_commit_records(&ident, 0, None)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LakeCatError::Internal(message)
                if message.contains(
                    "table commit record policy hash must be full SHA-256 evidence"
                )
        ));
    }
}

#[cfg(feature = "turso-local")]
pub mod turso_store {
    use std::{collections::BTreeSet, sync::Arc};

    use async_trait::async_trait;
    use chrono::Utc;
    use lakecat_core::{
        LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, TableName, WarehouseName,
        content_hash_bytes, content_hash_json,
    };
    use serde::de::DeserializeOwned;
    use serde_json::Value as JsonValue;
    use turso::{Connection, Database, Row, Value as TursoValue};

    use crate::{
        CatalogAuditEvent, CatalogStore, OutboxEvent, PolicyBinding, ProjectRecord, ServerRecord,
        SoftDeleteRecord, StorageProfile, TableCommit, TableCommitRecord, TableRecord, ViewRecord,
        ViewVersionReceipt, WarehouseRecord, metadata_pointer_conflict, namespace_not_empty,
        namespace_not_found, policy_binding_key, policy_bindings_for_table,
        require_expected_view_version, storage_profile_key, storage_profile_match, table_key,
        validate_expected_view_version, validate_project_id, validate_view_receipt_chains,
        view_key, view_key_parts, view_receipt_hash,
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
                    "select namespace_json, warehouse, namespace_path from namespaces
                     where warehouse = ?1
                     order by namespace_path",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut namespaces = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let namespace = decode_namespace(row_string(&row, 0)?)?;
                let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                let row_namespace_path = row_string(&row, 2)?;
                crate::validate_namespace_scope(
                    &namespace,
                    warehouse,
                    &row_warehouse,
                    row_namespace_path.as_str(),
                )?;
                namespaces.push(namespace);
            }
            Ok(namespaces)
        }

        async fn load_namespace(
            &self,
            warehouse: &WarehouseName,
            namespace: &Namespace,
        ) -> LakeCatResult<Namespace> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select namespace_json, warehouse, namespace_path from namespaces
                     where warehouse = ?1 and namespace_path = ?2",
                    (warehouse.as_str(), namespace.path()),
                )
                .await
                .map_err(turso_error)?;
            let Some(row) = rows.next().await.map_err(turso_error)? else {
                return Err(namespace_not_found(namespace));
            };
            let decoded = decode_namespace(row_string(&row, 0)?)?;
            let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
            let row_namespace_path = row_string(&row, 2)?;
            crate::validate_namespace_scope(
                &decoded,
                warehouse,
                &row_warehouse,
                row_namespace_path.as_str(),
            )?;
            Ok(decoded)
        }

        async fn drop_namespace(
            &self,
            warehouse: &WarehouseName,
            namespace: &Namespace,
        ) -> LakeCatResult<Namespace> {
            let namespace = self.load_namespace(warehouse, namespace).await?;
            let conn = self.connect()?;
            let namespace_path = namespace.path();
            if count_matching_rows(&conn, "tables", warehouse.as_str(), namespace_path.as_str())
                .await?
                > 0
            {
                return Err(namespace_not_empty(&namespace, "tables"));
            }
            if count_matching_rows(&conn, "views", warehouse.as_str(), namespace_path.as_str())
                .await?
                > 0
            {
                return Err(namespace_not_empty(&namespace, "views"));
            }
            if count_matching_rows(
                &conn,
                "policy_bindings",
                warehouse.as_str(),
                namespace_path.as_str(),
            )
            .await?
                > 0
            {
                return Err(namespace_not_empty(&namespace, "policy bindings"));
            }
            conn.execute(
                "delete from namespaces where warehouse = ?1 and namespace_path = ?2",
                (warehouse.as_str(), namespace_path),
            )
            .await
            .map_err(turso_error)?;
            Ok(namespace)
        }

        async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select record_json, table_key, warehouse, namespace_path, table_name
                     from tables t
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
                let table: TableRecord = decode_json(row_string(&row, 0)?)?;
                let ident = TableIdent::new(
                    WarehouseName::new(row_string(&row, 2)?)?,
                    row_string(&row, 3)?.parse()?,
                    TableName::new(row_string(&row, 4)?)?,
                );
                crate::validate_table_record_scope(
                    &table,
                    &ident,
                    &row_string(&row, 1)?,
                    &row_string(&row, 2)?,
                    &row_string(&row, 3)?,
                    &row_string(&row, 4)?,
                )?;
                tables.push(table);
            }
            Ok(tables)
        }

        async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord> {
            table.validate()?;
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
                    "select record_json, table_key, warehouse, namespace_path, table_name
                     from tables t
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
                .map(|row| {
                    let table: TableRecord = decode_json(row_string(&row, 0)?)?;
                    crate::validate_table_record_scope(
                        &table,
                        ident,
                        &row_string(&row, 1)?,
                        &row_string(&row, 2)?,
                        &row_string(&row, 3)?,
                        &row_string(&row, 4)?,
                    )?;
                    Ok(table)
                })
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
            commit.validate()?;
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
                        "select table_key, request_hash, response_json from idempotency_records where idem_key = ?1",
                        (idem_key,),
                    )
                    .await
                    .map_err(turso_error)?;
                if let Some(row) = rows.next().await.map_err(turso_error)? {
                    crate::validate_idempotency_record_table_key(&row_string(&row, 0)?, ident)?;
                    let replay_hash = row_string(&row, 1)?;
                    if replay_hash != idempotency_request_hash {
                        return Err(LakeCatError::Conflict(format!(
                            "idempotency key reused with different commit request for {}",
                            ident.stable_id()
                        )));
                    }
                    let table = decode_json(row_string(&row, 2)?)?;
                    crate::validate_table_record_identity(&table, ident)?;
                    tx.commit().await.map_err(turso_error)?;
                    return Ok(table);
                }
            }

            let mut rows = tx
                .query(
                    "select record_json, table_key, warehouse, namespace_path, table_name
                     from tables t
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
            crate::validate_table_record_scope(
                &table,
                ident,
                &row_string(&row, 1)?,
                &row_string(&row, 2)?,
                &row_string(&row, 3)?,
                &row_string(&row, 4)?,
            )?;
            let previous_metadata_location = table.metadata_location.clone();
            let idempotency_key_sha256 = commit
                .idempotency_key
                .as_ref()
                .map(|key| content_hash_bytes(key.as_bytes()));
            if previous_metadata_location != commit.expected_previous_metadata_location {
                return Err(metadata_pointer_conflict(
                    ident,
                    commit.expected_previous_metadata_location.as_deref(),
                    previous_metadata_location.as_deref(),
                ));
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
                return Err(metadata_pointer_conflict(
                    ident,
                    commit.expected_previous_metadata_location.as_deref(),
                    previous_metadata_location.as_deref(),
                ));
            }

            let record = TableCommitRecord {
                table: ident.clone(),
                previous_metadata_location,
                new_metadata_location: table.metadata_location.clone(),
                sequence_number: table.version,
                principal: commit.principal.clone(),
                format_version: crate::table_commit_format_version(&table),
                snapshot_id: crate::table_commit_snapshot_id(&table),
                policy_hash: crate::table_commit_policy_hash(commit.authorization_receipt.as_ref()),
                request_hash,
                response_hash: crate::table_response_hash(&table)?,
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

        async fn replay_table_commit(
            &self,
            ident: &TableIdent,
            idempotency_key: &str,
            idempotency_request_hash: &str,
        ) -> LakeCatResult<Option<TableRecord>> {
            crate::validate_idempotency_key_shape(idempotency_key)?;
            crate::validate_idempotency_request_hash_shape(idempotency_request_hash)?;
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select table_key, request_hash, response_json from idempotency_records where idem_key = ?1",
                    (idempotency_record_key(ident, idempotency_key),),
                )
                .await
                .map_err(turso_error)?;
            let Some(row) = rows.next().await.map_err(turso_error)? else {
                return Ok(None);
            };
            crate::validate_idempotency_record_table_key(&row_string(&row, 0)?, ident)?;
            let replay_hash = row_string(&row, 1)?;
            if replay_hash != idempotency_request_hash {
                return Err(LakeCatError::Conflict(format!(
                    "idempotency key reused with different commit request for {}",
                    ident.stable_id()
                )));
            }
            let table = decode_json(row_string(&row, 2)?)?;
            crate::validate_table_record_identity(&table, ident)?;
            Ok(Some(table))
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
                    "select record_json, table_key, warehouse, namespace_path, table_name
                     from tables t
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
            crate::validate_table_record_scope(
                &table,
                ident,
                &row_string(&row, 1)?,
                &row_string(&row, 2)?,
                &row_string(&row, 3)?,
                &row_string(&row, 4)?,
            )?;
            let deleted_at = Utc::now();
            let record = SoftDeleteRecord {
                table: ident.clone(),
                metadata_location: table.metadata_location.clone(),
                version: table.version,
                format_version: crate::table_commit_format_version(&table),
                principal: principal.clone(),
                authorization_receipt,
                deleted_at,
            };
            record.validate_for_table(ident, &table)?;
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
                    "select t.record_json, t.table_key, t.warehouse, t.namespace_path,
                            t.table_name, d.table_key, d.warehouse, d.namespace_path,
                            d.table_name, d.metadata_location, d.version,
                            d.deleted_at, d.record_json
                     from tables t
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
            crate::validate_table_record_scope(
                &table,
                ident,
                &row_string(&row, 1)?,
                &row_string(&row, 2)?,
                &row_string(&row, 3)?,
                &row_string(&row, 4)?,
            )?;
            let soft_delete: SoftDeleteRecord = decode_json(row_string(&row, 12)?)?;
            soft_delete.validate_for_table(ident, &table)?;
            validate_turso_soft_delete_row(&soft_delete, ident, &row, 5)?;
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
                "format-version": crate::table_commit_format_version(&table),
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
                    "select table_key, sequence_number, previous_metadata_location,
                            new_metadata_location, request_hash, principal_json, committed_at,
                            record_json
                     from metadata_pointer_log
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
                let commit: TableCommitRecord = decode_json(row_string(&row, 7)?)?;
                commit.validate_for_table(ident)?;
                validate_turso_commit_record_row(&commit, ident, &row)?;
                commits.push(commit);
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
                    "select record_json, server_id from servers
                     order by server_id",
                    (),
                )
                .await
                .map_err(turso_error)?;
            let mut servers = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let server: ServerRecord = decode_json(row_string(&row, 0)?)?;
                crate::validate_server_record_scope(&server, &row_string(&row, 1)?)?;
                servers.push(server);
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
                    "select record_json, project_id from projects
                     order by project_id",
                    (),
                )
                .await
                .map_err(turso_error)?;
            let mut projects = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let project: ProjectRecord = decode_json(row_string(&row, 0)?)?;
                crate::validate_project_record_scope(&project, &row_string(&row, 1)?)?;
                projects.push(project);
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
                    "select record_json, warehouse, project_id, storage_root from warehouses
                     where warehouse = ?1",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            rows.next()
                .await
                .map_err(turso_error)?
                .map(|row| {
                    let record: WarehouseRecord = decode_json(row_string(&row, 0)?)?;
                    let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                    crate::validate_warehouse_record_scope(
                        &record,
                        &row_warehouse,
                        &row_string(&row, 2)?,
                        row_optional_string(&row, 3)?.as_deref(),
                    )?;
                    Ok(record)
                })
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
                    "select record_json, warehouse, project_id, storage_root from warehouses
                     order by warehouse",
                    (),
                )
                .await
                .map_err(turso_error)?;
            let mut warehouses = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let record: WarehouseRecord = decode_json(row_string(&row, 0)?)?;
                let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                crate::validate_warehouse_record_scope(
                    &record,
                    &row_warehouse,
                    &row_string(&row, 2)?,
                    row_optional_string(&row, 3)?.as_deref(),
                )?;
                warehouses.push(record);
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
                    "select record_json, warehouse, project_id, storage_root from warehouses
                     where project_id = ?1
                     order by warehouse",
                    (project_id,),
                )
                .await
                .map_err(turso_error)?;
            let mut warehouses = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let record: WarehouseRecord = decode_json(row_string(&row, 0)?)?;
                let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                crate::validate_warehouse_record_scope(
                    &record,
                    &row_warehouse,
                    &row_string(&row, 2)?,
                    row_optional_string(&row, 3)?.as_deref(),
                )?;
                warehouses.push(record);
            }
            Ok(warehouses)
        }

        async fn upsert_view(&self, view: ViewRecord) -> LakeCatResult<ViewRecord> {
            self.upsert_view_if_version(view, None).await
        }

        async fn upsert_view_if_version(
            &self,
            view: ViewRecord,
            expected_view_version: Option<u64>,
        ) -> LakeCatResult<ViewRecord> {
            view.validate()?;
            if let Some(expected) = expected_view_version {
                validate_expected_view_version(expected)?;
            }
            let mut conn = self.connect()?;
            let tx = conn.transaction().await.map_err(turso_error)?;
            let view_key = view_key(&view);
            let principal = view.created.principal.clone();
            let previous = tx
                .query(
                    "select record_json, warehouse, namespace_path, view_name from views
                     where view_key = ?1
                     limit 1",
                    (view_key.as_str(),),
                )
                .await
                .map_err(turso_error)?
                .next()
                .await
                .map_err(turso_error)?
                .map(|row| {
                    let view = decode_json::<ViewRecord>(row_string(&row, 0)?)?;
                    let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                    let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
                    let row_view = TableName::new(row_string(&row, 3)?)?;
                    crate::validate_view_record_scope(
                        &view,
                        &row_warehouse,
                        &row_namespace,
                        &row_view,
                    )?;
                    Ok(view)
                })
                .transpose()?;
            if let Some(expected) = expected_view_version {
                require_expected_view_version(previous.as_ref(), expected)?;
            }
            let latest_receipt = latest_turso_view_receipt_evidence(
                &tx,
                view_key.as_str(),
                &view.warehouse,
                &view.namespace,
                &view.name,
            )
            .await?;
            let latest_receipt_version = latest_receipt
                .as_ref()
                .map(|(view_version, _)| *view_version);
            let previous_receipt_hash = latest_receipt.map(|(_, receipt_hash)| receipt_hash);
            let previous_view_version = previous
                .as_ref()
                .map(|view| view.view_version)
                .or(latest_receipt_version);
            let view =
                view.with_next_version_after_history(previous.as_ref(), latest_receipt_version)?;
            tx.execute(
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
            let receipt = ViewVersionReceipt::upsert(
                previous_view_version,
                previous_receipt_hash,
                &view,
                principal,
            )?;
            let receipt_id = view_receipt_hash(&receipt)?;
            let previous_view_version = receipt
                .previous_view_version
                .map(|version| checked_i64(version, "previous view version"))
                .transpose()?;
            tx.execute(
                "insert into view_version_receipts (
                    receipt_id, view_key, warehouse, namespace_path, view_name,
                    view_version, previous_view_version, operation, view_hash,
                    principal_json, receipt_json, recorded_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                (
                    receipt_id.as_str(),
                    view_key.as_str(),
                    receipt.warehouse.as_str(),
                    receipt.namespace.path().as_str(),
                    receipt.name.as_str(),
                    checked_i64(receipt.view_version, "view version")?,
                    previous_view_version,
                    "upsert",
                    receipt.view_hash.as_str(),
                    encode_json(&receipt.principal)?,
                    encode_json(&receipt)?,
                    receipt.recorded_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            tx.commit().await.map_err(turso_error)?;
            Ok(view)
        }

        async fn list_view_version_receipts(
            &self,
            warehouse: &WarehouseName,
            namespace: &Namespace,
            view: &TableName,
        ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
            let conn = self.connect()?;
            let view_key = view_key_parts(warehouse, namespace, view);
            let mut rows = conn
                .query(
                    "select receipt_json, warehouse, namespace_path, view_name from view_version_receipts
                     where view_key = ?1
                     order by view_version, recorded_at, receipt_id",
                    (view_key.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut receipts = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let receipt: ViewVersionReceipt = decode_json(row_string(&row, 0)?)?;
                let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
                let row_view = TableName::new(row_string(&row, 3)?)?;
                crate::validate_view_receipt_scope(
                    &receipt,
                    &row_warehouse,
                    &row_namespace,
                    Some(&row_view),
                )?;
                crate::validate_view_receipt_scope(&receipt, warehouse, namespace, Some(view))?;
                receipts.push(receipt);
            }
            validate_view_receipt_chains(&receipts)?;
            Ok(receipts)
        }

        async fn list_namespace_view_version_receipts(
            &self,
            warehouse: &WarehouseName,
            namespace: &Namespace,
        ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
            let conn = self.connect()?;
            let mut rows = conn
                .query(
                    "select receipt_json, view_name from view_version_receipts
                     where warehouse = ?1 and namespace_path = ?2
                     order by view_name, view_version, recorded_at, receipt_id",
                    (warehouse.as_str(), namespace.path().as_str()),
                )
                .await
                .map_err(turso_error)?;
            let mut receipts = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let receipt: ViewVersionReceipt = decode_json(row_string(&row, 0)?)?;
                let row_view = TableName::new(row_string(&row, 1)?)?;
                crate::validate_view_receipt_scope(
                    &receipt,
                    warehouse,
                    namespace,
                    Some(&row_view),
                )?;
                receipts.push(receipt);
            }
            validate_view_receipt_chains(&receipts)?;
            Ok(receipts)
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
                "select record_json, warehouse, namespace_path, view_name from views
                 where view_key = ?1
                 limit 1",
                (view_key.as_str(),),
            )
            .await
            .map_err(turso_error)?
            .next()
            .await
            .map_err(turso_error)?
            .map(|row| {
                let view = decode_json::<ViewRecord>(row_string(&row, 0)?)?;
                let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
                let row_view = TableName::new(row_string(&row, 3)?)?;
                crate::validate_view_record_scope(
                    &view,
                    &row_warehouse,
                    &row_namespace,
                    &row_view,
                )?;
                Ok(view)
            })
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
            principal: Principal,
        ) -> LakeCatResult<ViewRecord> {
            self.drop_view_if_version(warehouse, namespace, view, principal, None)
                .await
        }

        async fn drop_view_if_version(
            &self,
            warehouse: &WarehouseName,
            namespace: &Namespace,
            view: &TableName,
            principal: Principal,
            expected_view_version: Option<u64>,
        ) -> LakeCatResult<ViewRecord> {
            if let Some(expected) = expected_view_version {
                validate_expected_view_version(expected)?;
            }
            let mut conn = self.connect()?;
            let view_key = view_key_parts(warehouse, namespace, view);
            let tx = conn.transaction().await.map_err(turso_error)?;
            let record = tx
                .query(
                    "select record_json, warehouse, namespace_path, view_name from views
                     where view_key = ?1
                     limit 1",
                    (view_key.as_str(),),
                )
                .await
                .map_err(turso_error)?
                .next()
                .await
                .map_err(turso_error)?
                .map(|row| {
                    let view = decode_json::<ViewRecord>(row_string(&row, 0)?)?;
                    let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                    let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
                    let row_view = TableName::new(row_string(&row, 3)?)?;
                    crate::validate_view_record_scope(
                        &view,
                        &row_warehouse,
                        &row_namespace,
                        &row_view,
                    )?;
                    Ok(view)
                })
                .transpose()?
                .ok_or_else(|| LakeCatError::NotFound {
                    object: "view",
                    name: view.as_str().to_string(),
                })?;
            if let Some(expected) = expected_view_version {
                require_expected_view_version(Some(&record), expected)?;
            }
            let previous_receipt_hash =
                latest_turso_view_receipt_hash(&tx, view_key.as_str(), warehouse, namespace, view)
                    .await?;
            let receipt = ViewVersionReceipt::drop(&record, previous_receipt_hash, principal)?;
            let receipt_id = view_receipt_hash(&receipt)?;
            let previous_view_version = receipt
                .previous_view_version
                .map(|version| checked_i64(version, "previous view version"))
                .transpose()?;
            tx.execute(
                "delete from views where view_key = ?1",
                (view_key.as_str(),),
            )
            .await
            .map_err(turso_error)?;
            tx.execute(
                "insert into view_version_receipts (
                    receipt_id, view_key, warehouse, namespace_path, view_name,
                    view_version, previous_view_version, operation, view_hash,
                    principal_json, receipt_json, recorded_at
                 )
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                (
                    receipt_id.as_str(),
                    view_key.as_str(),
                    receipt.warehouse.as_str(),
                    receipt.namespace.path().as_str(),
                    receipt.name.as_str(),
                    checked_i64(receipt.view_version, "view version")?,
                    previous_view_version,
                    "drop",
                    receipt.view_hash.as_str(),
                    encode_json(&receipt.principal)?,
                    encode_json(&receipt)?,
                    receipt.recorded_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(turso_error)?;
            tx.commit().await.map_err(turso_error)?;
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
                    "select record_json, warehouse, namespace_path, view_name from views
                     where warehouse = ?1 and namespace_path = ?2
                     order by view_name",
                    (warehouse.as_str(), namespace.path().as_str()),
                )
                .await
                .map_err(turso_error)?;
            let mut views = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let view: ViewRecord = decode_json(row_string(&row, 0)?)?;
                let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
                let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
                let row_view = TableName::new(row_string(&row, 3)?)?;
                crate::validate_view_record_scope(
                    &view,
                    &row_warehouse,
                    &row_namespace,
                    &row_view,
                )?;
                views.push(view);
            }
            Ok(views)
        }

        async fn record_audit_event(&self, event: CatalogAuditEvent) -> LakeCatResult<()> {
            event.validate_recordable()?;
            let mut conn = self.connect()?;
            let tx = conn.transaction().await.map_err(turso_error)?;
            let event_id = crate::audit_event_id(&event)?;
            tx.execute(
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

            let outbox_payload = crate::audit_outbox_payload(&event_id, &event);
            tx_insert_outbox_event(&tx, &outbox_payload, event.created_at).await?;
            tx.commit().await.map_err(turso_error)?;
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
                let event = outbox_event_from_row(&row)?;
                event.validate_pending()?;
                events.push(event);
            }
            Ok(events)
        }

        async fn mark_outbox_delivered(&self, event_ids: &[String]) -> LakeCatResult<usize> {
            if event_ids.is_empty() {
                return Ok(0);
            }
            for event_id in event_ids {
                crate::validate_outbox_event_id_shape(event_id)?;
            }
            let event_ids = event_ids.iter().collect::<BTreeSet<_>>();
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
                    "select profile_json, profile_id, location_prefix, provider, issuance_mode
                     from storage_profiles
                     where warehouse = ?1
                     order by profile_id",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut profiles = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let profile: StorageProfile = decode_json(row_string(&row, 0)?)?;
                crate::validate_storage_profile_scope(
                    &profile,
                    warehouse,
                    &row_string(&row, 1)?,
                    &row_string(&row, 2)?,
                    &row_string(&row, 3)?,
                    &row_string(&row, 4)?,
                )?;
                profiles.push(profile);
            }
            Ok(profiles)
        }

        async fn storage_profile_for_table(
            &self,
            table: &TableRecord,
        ) -> LakeCatResult<StorageProfile> {
            let profiles = self.list_storage_profiles(&table.ident.warehouse).await?;
            Ok(storage_profile_match(profiles.iter(), table)?
                .unwrap_or_else(|| StorageProfile::inferred_for_table(table)))
        }

        async fn upsert_policy_binding(
            &self,
            binding: PolicyBinding,
        ) -> LakeCatResult<PolicyBinding> {
            binding.validate()?;
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
                    "select binding_json, policy_id, namespace_path, table_name, enforced
                     from policy_bindings
                     where warehouse = ?1
                     order by policy_id",
                    (warehouse.as_str(),),
                )
                .await
                .map_err(turso_error)?;
            let mut bindings = Vec::new();
            while let Some(row) = rows.next().await.map_err(turso_error)? {
                let binding: PolicyBinding = decode_json(row_string(&row, 0)?)?;
                crate::validate_policy_binding_scope(
                    &binding,
                    warehouse,
                    row_string(&row, 1)?.as_str(),
                    row_optional_string(&row, 2)?.as_deref(),
                    row_optional_string(&row, 3)?.as_deref(),
                    row_i64(&row, 4)? != 0,
                )?;
                bindings.push(binding);
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
        "create table if not exists view_version_receipts (
            receipt_id text primary key,
            view_key text not null,
            warehouse text not null,
            namespace_path text not null,
            view_name text not null,
            view_version integer not null,
            previous_view_version integer,
            operation text not null,
            view_hash text not null,
            principal_json text not null,
            receipt_json text not null,
            recorded_at text not null
        )",
        "create index if not exists idx_view_version_receipts_view
            on view_version_receipts (view_key, view_version)",
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

    fn row_i64(row: &Row, idx: usize) -> LakeCatResult<i64> {
        match row.get_value(idx).map_err(turso_error)? {
            TursoValue::Integer(value) => Ok(value),
            value => Err(LakeCatError::Internal(format!(
                "Turso catalog store expected integer at column {idx}, got {value:?}"
            ))),
        }
    }

    async fn latest_turso_view_receipt_evidence(
        tx: &turso::transaction::Transaction<'_>,
        view_key: &str,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<Option<(u64, String)>> {
        let mut rows = tx
            .query(
                "select receipt_json, warehouse, namespace_path, view_name from view_version_receipts
             where view_key = ?1
             order by view_version, recorded_at, receipt_id",
                (view_key,),
            )
            .await
            .map_err(turso_error)?;
        let mut receipts = Vec::new();
        while let Some(row) = rows.next().await.map_err(turso_error)? {
            let receipt = decode_json::<ViewVersionReceipt>(row_string(&row, 0)?)?;
            let row_warehouse = WarehouseName::new(row_string(&row, 1)?)?;
            let row_namespace = row_string(&row, 2)?.parse::<Namespace>()?;
            let row_view = TableName::new(row_string(&row, 3)?)?;
            crate::validate_view_receipt_scope(
                &receipt,
                &row_warehouse,
                &row_namespace,
                Some(&row_view),
            )?;
            crate::validate_view_receipt_scope(&receipt, warehouse, namespace, Some(view))?;
            receipts.push(receipt);
        }
        crate::latest_view_receipt_evidence(receipts.iter())
    }

    async fn latest_turso_view_receipt_hash(
        tx: &turso::transaction::Transaction<'_>,
        view_key: &str,
        warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<Option<String>> {
        latest_turso_view_receipt_evidence(tx, view_key, warehouse, namespace, view)
            .await
            .map(|evidence| evidence.map(|(_, hash)| hash))
    }

    async fn count_matching_rows(
        conn: &Connection,
        table: &str,
        warehouse: &str,
        namespace_path: &str,
    ) -> LakeCatResult<i64> {
        let sql = match table {
            "tables" => "select count(*) from tables where warehouse = ?1 and namespace_path = ?2",
            "views" => "select count(*) from views where warehouse = ?1 and namespace_path = ?2",
            "policy_bindings" => {
                "select count(*) from policy_bindings where warehouse = ?1 and namespace_path = ?2"
            }
            table => {
                return Err(LakeCatError::Internal(format!(
                    "unsupported Turso count table: {table}"
                )));
            }
        };
        let mut rows = conn
            .query(sql, (warehouse, namespace_path))
            .await
            .map_err(turso_error)?;
        let row = rows.next().await.map_err(turso_error)?.ok_or_else(|| {
            LakeCatError::Internal(format!("Turso catalog store returned no count for {table}"))
        })?;
        row_i64(&row, 0)
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

    fn validate_turso_commit_record_row(
        record: &TableCommitRecord,
        ident: &TableIdent,
        row: &Row,
    ) -> LakeCatResult<()> {
        if row_string(row, 0)? != table_key(ident) {
            return Err(LakeCatError::Internal(
                "table commit record row scope does not match requested table".to_string(),
            ));
        }
        let row_sequence_number = u64::try_from(row_i64(row, 1)?).map_err(|_| {
            LakeCatError::Internal(
                "Turso metadata pointer log sequence number must be positive".to_string(),
            )
        })?;
        if record.sequence_number != row_sequence_number {
            return Err(LakeCatError::Internal(
                "table commit record sequence number does not match pointer log row".to_string(),
            ));
        }
        if record.previous_metadata_location != row_optional_string(row, 2)? {
            return Err(LakeCatError::Internal(
                "table commit record previous metadata location does not match pointer log row"
                    .to_string(),
            ));
        }
        if record.new_metadata_location != row_optional_string(row, 3)? {
            return Err(LakeCatError::Internal(
                "table commit record new metadata location does not match pointer log row"
                    .to_string(),
            ));
        }
        if record.request_hash != row_string(row, 4)? {
            return Err(LakeCatError::Internal(
                "table commit record request hash does not match pointer log row".to_string(),
            ));
        }
        if record.principal != decode_json::<Principal>(row_string(row, 5)?)? {
            return Err(LakeCatError::Internal(
                "table commit record principal does not match pointer log row".to_string(),
            ));
        }
        if record.committed_at != parse_turso_datetime(row_string(row, 6)?, "commit committed_at")?
        {
            return Err(LakeCatError::Internal(
                "table commit record timestamp does not match pointer log row".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_turso_soft_delete_row(
        record: &SoftDeleteRecord,
        ident: &TableIdent,
        row: &Row,
        offset: usize,
    ) -> LakeCatResult<()> {
        if row_string(row, offset)? != table_key(ident)
            || row_string(row, offset + 1)? != record.table.warehouse.as_str()
            || row_string(row, offset + 2)? != record.table.namespace.path()
            || row_string(row, offset + 3)? != record.table.name.as_str()
        {
            return Err(LakeCatError::Internal(
                "soft-delete row scope does not match record identity".to_string(),
            ));
        }
        if record.metadata_location != row_optional_string(row, offset + 4)? {
            return Err(LakeCatError::Internal(
                "soft-delete metadata location does not match row".to_string(),
            ));
        }
        let row_version = u64::try_from(row_i64(row, offset + 5)?).map_err(|_| {
            LakeCatError::Internal("soft-delete row version must be non-negative".to_string())
        })?;
        if record.version != row_version {
            return Err(LakeCatError::Internal(
                "soft-delete version does not match row".to_string(),
            ));
        }
        if record.deleted_at
            != parse_turso_datetime(row_string(row, offset + 6)?, "soft-delete deleted_at")?
        {
            return Err(LakeCatError::Internal(
                "soft-delete timestamp does not match row".to_string(),
            ));
        }
        Ok(())
    }

    fn parse_turso_datetime(value: String, name: &str) -> LakeCatResult<chrono::DateTime<Utc>> {
        chrono::DateTime::parse_from_rfc3339(&value)
            .map(|datetime| datetime.with_timezone(&Utc))
            .map_err(|err| {
                LakeCatError::Internal(format!("failed to parse {name} timestamp: {err}"))
            })
    }

    async fn tx_insert_outbox_event(
        tx: &turso::transaction::Transaction<'_>,
        payload: &JsonValue,
        created_at: chrono::DateTime<Utc>,
    ) -> LakeCatResult<()> {
        tx.execute(
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

        use lakecat_core::{AuditStamp, Principal, TableName};

        use crate::{
            CredentialIssuanceMode, MemoryCatalogStore, PolicyBinding, ServerRecord,
            StorageProvider, ViewColumnRecord, ViewRecord, ViewVersionOperation,
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
        async fn turso_store_rejects_corrupt_server_records_on_read() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let mut record = ServerRecord::new(
                "lakecat-local",
                Some("Local LakeCat".to_string()),
                Some("http://127.0.0.1:8181".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_server(record.clone()).await.unwrap();
            record.endpoint_url = Some("http://127.0.0.1:8181?token=secret".to_string());

            let conn = store.connect().unwrap();
            conn.execute(
                "update servers set record_json = ?2 where server_id = ?1",
                ("lakecat-local", encode_json(&record).unwrap()),
            )
            .await
            .unwrap();

            let err = store.list_servers().await.unwrap_err();
            let message = err.to_string();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("server endpoint URL")
                        || message.contains("server-endpoint-url-hash=sha256:")
            ));
            assert!(!message.contains("token=secret"));
        }

        #[tokio::test]
        async fn turso_store_rejects_server_record_json_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let mut record = ServerRecord::new(
                "lakecat-local",
                Some("Local LakeCat".to_string()),
                Some("http://127.0.0.1:8181".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_server(record.clone()).await.unwrap();
            record.server_id = "lakecat-other".to_string();

            let conn = store.connect().unwrap();
            conn.execute(
                "update servers set record_json = ?2 where server_id = ?1",
                ("lakecat-local", encode_json(&record).unwrap()),
            )
            .await
            .unwrap();

            let err = store.list_servers().await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("server row scope does not match")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_server_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let record = ServerRecord::new(
                "lakecat-local",
                Some("Local LakeCat".to_string()),
                Some("http://127.0.0.1:8181".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_server(record).await.unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update servers set server_id = ?2 where server_id = ?1",
                ("lakecat-local", "lakecat-other"),
            )
            .await
            .unwrap();

            let err = store.list_servers().await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("server row scope does not match")
            ));
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
        async fn turso_store_rejects_corrupt_warehouse_records_on_read() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
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
            let mut record = WarehouseRecord::new(
                warehouse.clone(),
                "default",
                Some("file:///tmp/lakecat".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_warehouse(record.clone()).await.unwrap();
            record.storage_root = Some("file:///tmp/lakecat?token=secret".to_string());

            let conn = store.connect().unwrap();
            conn.execute(
                "update warehouses set record_json = ?2 where warehouse = ?1",
                ("local", encode_json(&record).unwrap()),
            )
            .await
            .unwrap();

            let err = store.load_warehouse(&warehouse).await.unwrap_err();
            let message = err.to_string();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("warehouse storage root")
                        || message.contains("warehouse-storage-root-hash=sha256:")
            ));
            assert!(!message.contains("token=secret"));

            let err = store.list_warehouses().await.unwrap_err();
            assert!(
                err.to_string()
                    .contains("warehouse-storage-root-hash=sha256:")
            );
        }

        #[tokio::test]
        async fn turso_store_rejects_warehouse_record_json_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
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
            let mut record = WarehouseRecord::new(
                warehouse.clone(),
                "default",
                Some("file:///tmp/lakecat".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_warehouse(record.clone()).await.unwrap();
            record.warehouse = WarehouseName::new("other").unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update warehouses set record_json = ?2 where warehouse = ?1",
                ("local", encode_json(&record).unwrap()),
            )
            .await
            .unwrap();

            for err in [
                store.load_warehouse(&warehouse).await.unwrap_err(),
                store.list_warehouses().await.unwrap_err(),
                store.list_project_warehouses("default").await.unwrap_err(),
            ] {
                assert!(matches!(
                    err,
                    LakeCatError::Internal(message)
                        if message.contains("warehouse row scope does not match")
                ));
            }
        }

        #[tokio::test]
        async fn turso_store_rejects_warehouse_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            store
                .upsert_project(
                    ProjectRecord::new(
                        "default",
                        None,
                        Some("Default Project".to_string()),
                        BTreeMap::new(),
                        Principal::anonymous(),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
            store
                .upsert_project(
                    ProjectRecord::new(
                        "other-project",
                        None,
                        Some("Other Project".to_string()),
                        BTreeMap::new(),
                        Principal::anonymous(),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let record = WarehouseRecord::new(
                warehouse.clone(),
                "default",
                Some("file:///tmp/lakecat".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_warehouse(record).await.unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update warehouses
                 set project_id = ?2, storage_root = ?3
                 where warehouse = ?1",
                ("local", "other-project", "file:///tmp/other-lakecat"),
            )
            .await
            .unwrap();

            for err in [
                store.load_warehouse(&warehouse).await.unwrap_err(),
                store.list_warehouses().await.unwrap_err(),
                store
                    .list_project_warehouses("other-project")
                    .await
                    .unwrap_err(),
            ] {
                assert!(matches!(
                    err,
                    LakeCatError::Internal(message)
                        if message.contains("warehouse row scope does not match")
                ));
            }
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
        async fn turso_store_rejects_corrupt_project_records_on_read() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
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
            let mut project = ProjectRecord::new(
                "default",
                Some("lakecat-local".to_string()),
                Some("QueryGraph Project".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_project(project.clone()).await.unwrap();
            project.server_id = Some("lakecat-local?token=secret".to_string());

            let conn = store.connect().unwrap();
            conn.execute(
                "update projects set record_json = ?2 where project_id = ?1",
                ("default", encode_json(&project).unwrap()),
            )
            .await
            .unwrap();

            let err = store.list_projects().await.unwrap_err();
            let message = err.to_string();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("project") || message.contains("identifier")
            ));
            assert!(!message.contains("token=secret"));
        }

        #[tokio::test]
        async fn turso_store_rejects_project_record_json_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let mut project = ProjectRecord::new(
                "default",
                None,
                Some("QueryGraph Project".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_project(project.clone()).await.unwrap();
            project.project_id = "other-project".to_string();

            let conn = store.connect().unwrap();
            conn.execute(
                "update projects set record_json = ?2 where project_id = ?1",
                ("default", encode_json(&project).unwrap()),
            )
            .await
            .unwrap();

            let err = store.list_projects().await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("project row scope does not match")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_project_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let project = ProjectRecord::new(
                "default",
                None,
                Some("QueryGraph Project".to_string()),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_project(project).await.unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update projects set project_id = ?2 where project_id = ?1",
                ("default", "other-project"),
            )
            .await
            .unwrap();

            let err = store.list_projects().await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("project row scope does not match")
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
            let view = store.upsert_view(view).await.unwrap();
            assert_eq!(view.view_version, 1);

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
            .unwrap()
            .with_columns(vec![ViewColumnRecord {
                name: "id".to_string(),
                data_type: serde_json::json!("long"),
                nullable: false,
                comment: Some("Customer identifier".to_string()),
            }])
            .unwrap();
            let updated = store
                .upsert_view_if_version(updated, Some(1))
                .await
                .unwrap();
            assert_eq!(updated.view_version, 2);
            let stale = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("active_customers").unwrap(),
                "select id from customers where active",
                "sql",
                Some(3),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            let err = store
                .upsert_view_if_version(stale, Some(1))
                .await
                .expect_err("stale expected view version must conflict");
            assert!(matches!(
                err,
                LakeCatError::Conflict(message) if message.contains("expected version 1")
            ));
            let receipts = store
                .list_view_version_receipts(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(receipts.len(), 2);
            assert_eq!(
                receipts[0].stable_id,
                "lakecat:view:local:default:active_customers"
            );
            assert_eq!(receipts[0].view_version, 1);
            assert_eq!(receipts[0].previous_view_version, None);
            assert_eq!(receipts[0].previous_receipt_hash, None);
            assert_eq!(receipts[0].operation, ViewVersionOperation::Upsert);
            assert!(!receipts[0].view_hash.is_empty());
            let first_receipt_hash = view_receipt_hash(&receipts[0]).unwrap();
            assert_eq!(receipts[1].view_version, 2);
            assert_eq!(receipts[1].previous_view_version, Some(1));
            assert_eq!(
                receipts[1].previous_receipt_hash.as_deref(),
                Some(first_receipt_hash.as_str())
            );
            assert_ne!(receipts[0].view_hash, receipts[1].view_hash);
            let second_receipt_hash = view_receipt_hash(&receipts[1]).unwrap();

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
            let err = store
                .drop_view_if_version(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap(),
                    Principal::anonymous(),
                    Some(1),
                )
                .await
                .expect_err("stale expected view version must not drop the view");
            assert!(matches!(
                err,
                LakeCatError::Conflict(message) if message.contains("expected version 1")
            ));
            assert_eq!(
                store
                    .list_view_version_receipts(
                        &warehouse,
                        &namespace,
                        &TableName::new("active_customers").unwrap(),
                    )
                    .await
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(
                store
                    .drop_view_if_version(
                        &warehouse,
                        &namespace,
                        &TableName::new("active_customers").unwrap(),
                        Principal::anonymous(),
                        Some(2)
                    )
                    .await
                    .unwrap(),
                updated
            );
            let receipts = store
                .list_view_version_receipts(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(receipts.len(), 3);
            assert_eq!(receipts[2].stable_id, receipts[1].stable_id);
            assert_eq!(receipts[2].view_version, 2);
            assert_eq!(receipts[2].previous_view_version, Some(2));
            assert_eq!(
                receipts[2].previous_receipt_hash.as_deref(),
                Some(second_receipt_hash.as_str())
            );
            assert_eq!(receipts[2].operation, ViewVersionOperation::Drop);
            assert_eq!(receipts[2].view_hash, receipts[1].view_hash);
            let namespace_receipts = store
                .list_namespace_view_version_receipts(&warehouse, &namespace)
                .await
                .unwrap();
            assert_eq!(namespace_receipts, receipts);
            assert_eq!(
                store.list_views(&warehouse, &namespace).await.unwrap(),
                Vec::<ViewRecord>::new()
            );
            assert!(matches!(
                store
                    .drop_view(
                        &warehouse,
                        &namespace,
                        &TableName::new("active_customers").unwrap(),
                        Principal::anonymous()
                    )
                    .await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "view" && name == "active_customers"
            ));
            let recreated = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("active_customers").unwrap(),
                "select id from customers where active",
                "sql",
                Some(3),
                BTreeMap::from([("owner".to_string(), "lakecat".to_string())]),
                Principal::anonymous(),
            )
            .unwrap();
            let recreated = store.upsert_view(recreated).await.unwrap();
            assert_eq!(recreated.view_version, 3);
            let receipts = store
                .list_view_version_receipts(
                    &warehouse,
                    &namespace,
                    &TableName::new("active_customers").unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(receipts.len(), 4);
            let drop_receipt_hash = view_receipt_hash(&receipts[2]).unwrap();
            assert_eq!(receipts[3].stable_id, receipts[2].stable_id);
            assert_eq!(receipts[3].view_version, 3);
            assert_eq!(receipts[3].previous_view_version, Some(2));
            assert_eq!(
                receipts[3].previous_receipt_hash.as_deref(),
                Some(drop_receipt_hash.as_str())
            );
            assert_eq!(receipts[3].operation, ViewVersionOperation::Upsert);
            assert_ne!(receipts[3].view_hash, receipts[2].view_hash);
            assert_eq!(
                store
                    .load_view(
                        &warehouse,
                        &namespace,
                        &TableName::new("active_customers").unwrap()
                    )
                    .await
                    .unwrap(),
                recreated
            );
        }

        #[tokio::test]
        async fn turso_store_rejects_corrupt_view_records_on_read() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let view_name = TableName::new("active_customers").unwrap();
            let view = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select * from customers where active",
                "sql",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            let mut view = store.upsert_view(view).await.unwrap();
            view.sql = "   ".to_string();

            let conn = store.connect().unwrap();
            conn.execute(
                "update views set record_json = ?2 where view_key = ?1",
                (view_key(&view), encode_json(&view).unwrap()),
            )
            .await
            .unwrap();

            let err = store
                .load_view(&warehouse, &namespace, &view_name)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("view SQL must not be empty")
            ));

            let err = store.list_views(&warehouse, &namespace).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("view SQL must not be empty")
            ));

            let err = store
                .drop_view(&warehouse, &namespace, &view_name, Principal::anonymous())
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("view SQL must not be empty")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_view_record_json_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let view_name = TableName::new("active_customers").unwrap();
            let view = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select * from customers where active",
                "sql",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            let mut view = store.upsert_view(view).await.unwrap();
            let original_view_key = view_key(&view);
            view.name = TableName::new("other_view").unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update views set record_json = ?2 where view_key = ?1",
                (original_view_key.as_str(), encode_json(&view).unwrap()),
            )
            .await
            .unwrap();

            let replacement = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select id from customers where active",
                "sql",
                Some(2),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            for err in [
                store
                    .load_view(&warehouse, &namespace, &view_name)
                    .await
                    .unwrap_err(),
                store.list_views(&warehouse, &namespace).await.unwrap_err(),
                store
                    .upsert_view_if_version(replacement, Some(1))
                    .await
                    .unwrap_err(),
                store
                    .drop_view(&warehouse, &namespace, &view_name, Principal::anonymous())
                    .await
                    .unwrap_err(),
            ] {
                assert!(matches!(
                    err,
                    LakeCatError::Internal(message)
                        if message.contains("view record row scope does not match")
                ));
            }
        }

        #[tokio::test]
        async fn turso_store_rejects_view_record_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let view_name = TableName::new("active_customers").unwrap();
            let view = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select * from customers where active",
                "sql",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            let view = store.upsert_view(view).await.unwrap();
            let original_view_key = view_key(&view);

            let conn = store.connect().unwrap();
            conn.execute(
                "update views
                 set namespace_path = ?2, view_name = ?3
                 where view_key = ?1",
                (
                    original_view_key.as_str(),
                    "tenant_shadow",
                    "shadow_active_customers",
                ),
            )
            .await
            .unwrap();

            let replacement = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select id from customers where active",
                "sql",
                Some(2),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            for err in [
                store
                    .load_view(&warehouse, &namespace, &view_name)
                    .await
                    .unwrap_err(),
                store
                    .list_views(&warehouse, &"tenant_shadow".parse::<Namespace>().unwrap())
                    .await
                    .unwrap_err(),
                store
                    .upsert_view_if_version(replacement, Some(1))
                    .await
                    .unwrap_err(),
                store
                    .drop_view(&warehouse, &namespace, &view_name, Principal::anonymous())
                    .await
                    .unwrap_err(),
            ] {
                assert!(matches!(
                    err,
                    LakeCatError::Internal(message)
                        if message.contains("view record row scope does not match")
                ));
            }
        }

        #[tokio::test]
        async fn turso_store_rejects_corrupt_view_receipts_on_read() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let view_name = TableName::new("active_customers").unwrap();
            let view = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select * from customers where active",
                "sql",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_view(view).await.unwrap();

            let mut receipts = store
                .list_view_version_receipts(&warehouse, &namespace, &view_name)
                .await
                .unwrap();
            let receipt_id = view_receipt_hash(&receipts[0]).unwrap();
            receipts[0].view_hash = "sha256:short".to_string();
            let conn = store.connect().unwrap();
            conn.execute(
                "update view_version_receipts set receipt_json = ?2 where receipt_id = ?1",
                (receipt_id, encode_json(&receipts[0]).unwrap()),
            )
            .await
            .unwrap();

            let err = store
                .list_view_version_receipts(&warehouse, &namespace, &view_name)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt hash must be a SHA-256 digest")
            ));

            let err = store
                .list_namespace_view_version_receipts(&warehouse, &namespace)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt hash must be a SHA-256 digest")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_corrupt_view_receipt_chain_links_on_read() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let view_name = TableName::new("active_customers").unwrap();
            let view = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select * from customers where active",
                "sql",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_view(view).await.unwrap();
            let updated = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select id from customers where active",
                "sql",
                Some(2),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store
                .upsert_view_if_version(updated, Some(1))
                .await
                .unwrap();

            let mut receipts = store
                .list_view_version_receipts(&warehouse, &namespace, &view_name)
                .await
                .unwrap();
            let receipt_id = view_receipt_hash(&receipts[1]).unwrap();
            receipts[1].previous_receipt_hash =
                Some(content_hash_json(&serde_json::json!({"forged": "previous"})).unwrap());
            let conn = store.connect().unwrap();
            conn.execute(
                "update view_version_receipts set receipt_json = ?2 where receipt_id = ?1",
                (receipt_id, encode_json(&receipts[1]).unwrap()),
            )
            .await
            .unwrap();

            let err = store
                .list_view_version_receipts(&warehouse, &namespace, &view_name)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt chain previous links must match")
            ));

            let err = store
                .list_namespace_view_version_receipts(&warehouse, &namespace)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt chain previous links must match")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_corrupt_view_receipt_chain_before_mutation() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let view_name = TableName::new("active_customers").unwrap();
            let view = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select * from customers where active",
                "sql",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_view(view).await.unwrap();
            let updated = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select id from customers where active",
                "sql",
                Some(2),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store
                .upsert_view_if_version(updated, Some(1))
                .await
                .unwrap();

            let mut receipts = store
                .list_view_version_receipts(&warehouse, &namespace, &view_name)
                .await
                .unwrap();
            let receipt_id = view_receipt_hash(&receipts[1]).unwrap();
            receipts[1].previous_receipt_hash =
                Some(content_hash_json(&serde_json::json!({"forged": "previous"})).unwrap());
            let conn = store.connect().unwrap();
            conn.execute(
                "update view_version_receipts set receipt_json = ?2 where receipt_id = ?1",
                (receipt_id, encode_json(&receipts[1]).unwrap()),
            )
            .await
            .unwrap();

            let attempted = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select id, email from customers where active",
                "sql",
                Some(3),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            let err = store
                .upsert_view_if_version(attempted, Some(2))
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt chain previous links must match")
            ));

            let active = store
                .load_view(&warehouse, &namespace, &view_name)
                .await
                .unwrap();
            assert_eq!(active.view_version, 2);
            assert_eq!(store.count_rows("view_version_receipts").await.unwrap(), 2);
        }

        #[tokio::test]
        async fn turso_store_rejects_view_receipt_json_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let view_name = TableName::new("active_customers").unwrap();
            let view = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select * from customers where active",
                "sql",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_view(view).await.unwrap();
            let updated = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select id from customers where active",
                "sql",
                Some(2),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store
                .upsert_view_if_version(updated, Some(1))
                .await
                .unwrap();

            let mut receipts = store
                .list_view_version_receipts(&warehouse, &namespace, &view_name)
                .await
                .unwrap();
            let receipt_id = view_receipt_hash(&receipts[1]).unwrap();
            let other_view_name = TableName::new("other_customers").unwrap();
            receipts[1].name = other_view_name.clone();
            receipts[1].stable_id = format!(
                "lakecat:view:{}:{}:{}",
                warehouse.as_str(),
                namespace.path(),
                other_view_name.as_str()
            );
            let conn = store.connect().unwrap();
            conn.execute(
                "update view_version_receipts set receipt_json = ?2 where receipt_id = ?1",
                (receipt_id, encode_json(&receipts[1]).unwrap()),
            )
            .await
            .unwrap();

            let err = store
                .list_view_version_receipts(&warehouse, &namespace, &view_name)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt row scope does not match")
            ));
            let err = store
                .list_namespace_view_version_receipts(&warehouse, &namespace)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt row scope does not match")
            ));

            let attempted = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select id, email from customers where active",
                "sql",
                Some(3),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            let err = store
                .upsert_view_if_version(attempted, Some(2))
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt row scope does not match")
            ));
            assert_eq!(
                store
                    .load_view(&warehouse, &namespace, &view_name)
                    .await
                    .unwrap()
                    .view_version,
                2
            );
            assert_eq!(store.count_rows("view_version_receipts").await.unwrap(), 2);
        }

        #[tokio::test]
        async fn turso_store_rejects_view_receipt_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let shadow_namespace = "tenant_shadow".parse::<Namespace>().unwrap();
            let view_name = TableName::new("active_customers").unwrap();
            let shadow_view_name = TableName::new("shadow_active_customers").unwrap();
            let view = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select * from customers where active",
                "sql",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store.upsert_view(view).await.unwrap();
            let updated = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select id from customers where active",
                "sql",
                Some(2),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            store
                .upsert_view_if_version(updated, Some(1))
                .await
                .unwrap();

            let receipts = store
                .list_view_version_receipts(&warehouse, &namespace, &view_name)
                .await
                .unwrap();
            let receipt_id = view_receipt_hash(&receipts[1]).unwrap();
            let conn = store.connect().unwrap();
            conn.execute(
                "update view_version_receipts
                 set namespace_path = ?2, view_name = ?3
                 where receipt_id = ?1",
                (
                    receipt_id.as_str(),
                    shadow_namespace.path().as_str(),
                    shadow_view_name.as_str(),
                ),
            )
            .await
            .unwrap();

            let err = store
                .list_view_version_receipts(&warehouse, &namespace, &view_name)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt row scope does not match")
            ));
            let err = store
                .list_namespace_view_version_receipts(&warehouse, &shadow_namespace)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt row scope does not match")
            ));

            let attempted = ViewRecord::new(
                warehouse.clone(),
                namespace.clone(),
                view_name.clone(),
                "select id, email from customers where active",
                "sql",
                Some(3),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .unwrap();
            let err = store
                .upsert_view_if_version(attempted, Some(2))
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("view receipt row scope does not match")
            ));
            assert_eq!(
                store
                    .load_view(&warehouse, &namespace, &view_name)
                    .await
                    .unwrap()
                    .view_version,
                2
            );
            assert_eq!(store.count_rows("view_version_receipts").await.unwrap(), 2);
        }

        #[tokio::test]
        async fn turso_store_loads_and_drops_namespaces() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "empty".parse::<Namespace>().unwrap();

            assert!(matches!(
                store.load_namespace(&warehouse, &namespace).await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "namespace" && name == "empty"
            ));
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            assert_eq!(
                store.load_namespace(&warehouse, &namespace).await.unwrap(),
                namespace.clone()
            );
            assert_eq!(
                store.drop_namespace(&warehouse, &namespace).await.unwrap(),
                namespace
            );
            assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);

            let occupied_namespace = "occupied".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                occupied_namespace.clone(),
                TableName::new("events").unwrap(),
            );
            let table = TableRecord::new(
                ident,
                "file:///tmp/occupied".to_string(),
                Some("file:///tmp/occupied/metadata/00000.json".to_string()),
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            store.create_table(table).await.unwrap();
            assert!(matches!(
                store.drop_namespace(&warehouse, &occupied_namespace).await,
                Err(LakeCatError::Conflict(message)) if message.contains("tables")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_namespace_json_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();

            let drifted = "other".parse::<Namespace>().unwrap();
            let conn = store.connect().unwrap();
            conn.execute(
                "update namespaces set namespace_json = ?3 where warehouse = ?1 and namespace_path = ?2",
                (
                    warehouse.as_str(),
                    namespace.path().as_str(),
                    encode_json(drifted.parts()).unwrap(),
                ),
            )
            .await
            .unwrap();

            for err in [
                store.list_namespaces(&warehouse).await.unwrap_err(),
                store
                    .load_namespace(&warehouse, &namespace)
                    .await
                    .unwrap_err(),
                store
                    .drop_namespace(&warehouse, &namespace)
                    .await
                    .unwrap_err(),
            ] {
                assert!(matches!(
                    err,
                    LakeCatError::Internal(message)
                        if message.contains("namespace row scope does not match")
                ));
            }
        }

        #[tokio::test]
        async fn turso_store_rejects_namespace_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let shadow_namespace = "tenant_shadow".parse::<Namespace>().unwrap();
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update namespaces
                 set namespace_path = ?3
                 where warehouse = ?1 and namespace_path = ?2",
                (
                    warehouse.as_str(),
                    namespace.path().as_str(),
                    shadow_namespace.path().as_str(),
                ),
            )
            .await
            .unwrap();

            for err in [
                store.list_namespaces(&warehouse).await.unwrap_err(),
                store
                    .load_namespace(&warehouse, &shadow_namespace)
                    .await
                    .unwrap_err(),
                store
                    .drop_namespace(&warehouse, &shadow_namespace)
                    .await
                    .unwrap_err(),
            ] {
                assert!(matches!(
                    err,
                    LakeCatError::Internal(message)
                        if message.contains("namespace row scope does not match")
                ));
            }
        }

        #[tokio::test]
        async fn turso_store_rejects_deserialized_empty_table_locations() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            let table = TableRecord {
                ident: ident.clone(),
                location: "   ".to_string(),
                metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
                metadata: serde_json::json!({"format-version": 3}),
                created: AuditStamp::now(Principal::anonymous()),
                updated_at: Utc::now(),
                version: 0,
            };

            let err = store.create_table(table).await.unwrap_err();

            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table location must not be empty")
            ));
            assert!(matches!(
                store.load_table(&ident).await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "table" && name == ident.stable_id()
            ));
            assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);
        }

        #[tokio::test]
        async fn turso_store_rejects_deserialized_invalid_table_metadata() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            let base = TableRecord {
                ident: ident.clone(),
                location: "file:///tmp/events".to_string(),
                metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
                metadata: serde_json::json!({"format-version": 3}),
                created: AuditStamp::now(Principal::anonymous()),
                updated_at: Utc::now(),
                version: 0,
            };

            let mut empty_metadata_location = base.clone();
            empty_metadata_location.metadata_location = Some("  ".to_string());
            let err = store
                .create_table(empty_metadata_location)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table metadata location must not be empty")
            ));

            let mut non_object_metadata = base;
            non_object_metadata.metadata = serde_json::json!("not metadata");
            let err = store.create_table(non_object_metadata).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table metadata must be a JSON object")
            ));

            let mut missing_format_version = TableRecord {
                ident: ident.clone(),
                location: "file:///tmp/events".to_string(),
                metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
                metadata: serde_json::json!({"current-snapshot-id": 42}),
                created: AuditStamp::now(Principal::anonymous()),
                updated_at: Utc::now(),
                version: 0,
            };
            let err = store
                .create_table(missing_format_version.clone())
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table metadata format-version must be present")
            ));

            missing_format_version.metadata = serde_json::json!({"format-version": 0});
            let err = store
                .create_table(missing_format_version)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table metadata format-version must be positive")
            ));

            assert!(matches!(
                store.load_table(&ident).await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "table" && name == ident.stable_id()
            ));
            assert_eq!(store.list_namespaces(&warehouse).await.unwrap(), vec![]);
        }

        #[tokio::test]
        async fn turso_store_rejects_deserialized_invalid_table_commits() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                ))
                .await
                .unwrap();

            let base_commit = TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"format-version": 3})),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            };

            let mut blank_idempotency_key = base_commit.clone();
            blank_idempotency_key.idempotency_key = Some("  ".to_string());
            let err = store
                .commit_table(&ident, blank_idempotency_key)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table commit idempotency key may only contain")
            ));

            let mut request_hash_without_key = base_commit.clone();
            request_hash_without_key.idempotency_request_hash =
                Some(content_hash_bytes("commit-request".as_bytes()));
            let err = store
                .commit_table(&ident, request_hash_without_key)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains(
                        "table commit idempotency request hash requires an idempotency key"
                    )
            ));

            let mut malformed_request_hash = base_commit.clone();
            malformed_request_hash.idempotency_key = Some("commit-1".to_string());
            malformed_request_hash.idempotency_request_hash = Some("sha256:short".to_string());
            let err = store
                .commit_table(&ident, malformed_request_hash)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains(
                        "table commit idempotency request hash must be full SHA-256 evidence"
                    )
            ));

            let err = store
                .replay_table_commit(
                    &ident,
                    " ",
                    &content_hash_bytes("commit-request".as_bytes()),
                )
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table commit idempotency key may only contain")
            ));

            let err = store
                .replay_table_commit(&ident, "commit-1", "sha256:short")
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains(
                        "table commit idempotency request hash must be full SHA-256 evidence"
                    )
            ));
            let table = store.load_table(&ident).await.unwrap();
            assert_eq!(table.version, 0);
            assert_eq!(
                table.metadata_location.as_deref(),
                Some("file:///tmp/events/metadata/00000.json")
            );
            assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
            assert_eq!(store.count_rows("idempotency_records").await.unwrap(), 0);

            let mut empty_expected_location = base_commit.clone();
            empty_expected_location.expected_previous_metadata_location = Some("  ".to_string());
            let err = store
                .commit_table(&ident, empty_expected_location)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("expected table metadata location must not be empty")
            ));

            let mut empty_new_location = base_commit.clone();
            empty_new_location.new_metadata_location = Some("  ".to_string());
            let err = store
                .commit_table(&ident, empty_new_location)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("new table metadata location must not be empty")
            ));

            let mut non_object_metadata = base_commit;
            non_object_metadata.new_metadata = Some(serde_json::json!("not metadata"));
            let err = store
                .commit_table(&ident, non_object_metadata)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("new table metadata must be a JSON object")
            ));

            let missing_format_version = TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"current-snapshot-id": 42})),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            };
            let err = store
                .commit_table(&ident, missing_format_version)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("new table metadata format-version must be present")
            ));

            let zero_format_version = TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"format-version": 0})),
                idempotency_key: None,
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            };
            let err = store
                .commit_table(&ident, zero_format_version)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("new table metadata format-version must be positive")
            ));

            let table = store.load_table(&ident).await.unwrap();
            assert_eq!(table.version, 0);
            assert_eq!(
                table.metadata_location.as_deref(),
                Some("file:///tmp/events/metadata/00000.json")
            );
            assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
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
            let message = err.to_string();
            assert!(message.contains("idempotency key reused with different commit request"));
            assert!(!message.contains("commit-1"));
            assert!(!message.contains("00002.json"));
            assert!(!message.contains("file:///tmp/events/metadata/00002.json"));

            let commit_count = store.count_rows("metadata_pointer_log").await.unwrap();
            assert_eq!(commit_count, 1);
            let commit_records = store.table_commit_records(&ident, 1, None).await.unwrap();
            assert_eq!(commit_records.len(), 1);
            assert_eq!(commit_records[0].sequence_number, 1);
            let replayed_probe = store
                .replay_table_commit(&ident, "commit-1", &commit_records[0].request_hash)
                .await
                .unwrap()
                .expect("idempotency replay should be available before commit planning");
            assert_eq!(replayed_probe.version, 1);
            assert_eq!(
                commit_records[0].response_hash,
                crate::table_response_hash(&replayed_probe).unwrap()
            );
            assert_eq!(commit_records[0].format_version, Some(3));
            assert_eq!(commit_records[0].snapshot_id, Some(0));
            assert_eq!(commit_records[0].policy_hash, None);
            let different_request_hash = content_hash_bytes("different-request".as_bytes());
            let replay_mismatch = store
                .replay_table_commit(&ident, "commit-1", &different_request_hash)
                .await
                .unwrap_err();
            let message = replay_mismatch.to_string();
            assert!(message.contains("idempotency key reused with different commit request"));
            assert!(!message.contains("commit-1"));
            assert!(!message.contains(different_request_hash.as_str()));
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
            assert_eq!(
                pending[0].payload["commit"]["response_hash"],
                serde_json::json!(commit_records[0].response_hash)
            );
            assert_eq!(
                pending[0].payload["commit"]["format_version"],
                serde_json::json!(3)
            );
            assert_eq!(
                pending[0].payload["commit"]["snapshot_id"],
                serde_json::json!(0)
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
        async fn turso_store_rejects_malformed_commit_history_records() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                ))
                .await
                .unwrap();
            store
                .commit_table(
                    &ident,
                    TableCommit {
                        requirements: vec![],
                        updates: vec![serde_json::json!({"action": "noop"})],
                        expected_previous_metadata_location: Some(
                            "file:///tmp/events/metadata/00000.json".to_string(),
                        ),
                        new_metadata_location: Some(
                            "file:///tmp/events/metadata/00001.json".to_string(),
                        ),
                        new_metadata: Some(serde_json::json!({"format-version": 3})),
                        idempotency_key: Some("commit-1".to_string()),
                        idempotency_request_hash: None,
                        principal: Principal::anonymous(),
                        authorization_receipt: None,
                    },
                )
                .await
                .unwrap();
            let mut records = store.table_commit_records(&ident, 1, None).await.unwrap();
            assert_eq!(records.len(), 1);
            let base_record = records.remove(0);
            let mut malformed_idempotency_record = base_record.clone();
            malformed_idempotency_record.idempotency_key_sha256 = Some("sha256:short".to_string());

            let conn = store.connect().unwrap();
            conn.execute(
                "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
                (
                    table_key(&ident),
                    encode_json(&malformed_idempotency_record).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store
                .table_commit_records(&ident, 0, None)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains(
                        "table commit record idempotency key hash must be full SHA-256 evidence"
                    )
            ));

            let mut malformed_policy_record = base_record;
            malformed_policy_record.policy_hash = Some("sha256:short".to_string());

            let conn = store.connect().unwrap();
            conn.execute(
                "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
                (
                    table_key(&ident),
                    encode_json(&malformed_policy_record).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store
                .table_commit_records(&ident, 0, None)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains(
                        "table commit record policy hash must be full SHA-256 evidence"
                    )
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_commit_history_row_json_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                ))
                .await
                .unwrap();
            store
                .commit_table(
                    &ident,
                    TableCommit {
                        requirements: vec![],
                        updates: vec![serde_json::json!({"action": "noop"})],
                        expected_previous_metadata_location: Some(
                            "file:///tmp/events/metadata/00000.json".to_string(),
                        ),
                        new_metadata_location: Some(
                            "file:///tmp/events/metadata/00001.json".to_string(),
                        ),
                        new_metadata: Some(serde_json::json!({"format-version": 3})),
                        idempotency_key: None,
                        idempotency_request_hash: None,
                        principal: Principal::anonymous(),
                        authorization_receipt: None,
                    },
                )
                .await
                .unwrap();
            let mut records = store.table_commit_records(&ident, 1, None).await.unwrap();
            assert_eq!(records.len(), 1);
            records[0].sequence_number = 2;

            let conn = store.connect().unwrap();
            conn.execute(
                "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
                (table_key(&ident), encode_json(&records[0]).unwrap()),
            )
            .await
            .unwrap();

            let err = store
                .table_commit_records(&ident, 0, None)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains(
                        "table commit record sequence number does not match pointer log row"
                    )
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_commit_history_record_table_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                ))
                .await
                .unwrap();
            store
                .commit_table(
                    &ident,
                    TableCommit {
                        requirements: vec![],
                        updates: vec![serde_json::json!({"action": "noop"})],
                        expected_previous_metadata_location: Some(
                            "file:///tmp/events/metadata/00000.json".to_string(),
                        ),
                        new_metadata_location: Some(
                            "file:///tmp/events/metadata/00001.json".to_string(),
                        ),
                        new_metadata: Some(serde_json::json!({"format-version": 3})),
                        idempotency_key: None,
                        idempotency_request_hash: None,
                        principal: Principal::anonymous(),
                        authorization_receipt: None,
                    },
                )
                .await
                .unwrap();
            let mut records = store.table_commit_records(&ident, 1, None).await.unwrap();
            assert_eq!(records.len(), 1);
            records[0].table = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("other_events").unwrap(),
            );

            let conn = store.connect().unwrap();
            conn.execute(
                "update metadata_pointer_log set record_json = ?2 where table_key = ?1",
                (table_key(&ident), encode_json(&records[0]).unwrap()),
            )
            .await
            .unwrap();

            let err = store
                .table_commit_records(&ident, 0, None)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("table commit record table does not match requested table")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_commit_history_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            let other_ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("other_events").unwrap(),
            );
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            for table_ident in [&ident, &other_ident] {
                store
                    .create_table(TableRecord::new(
                        table_ident.clone(),
                        format!("file:///tmp/{}", table_ident.name),
                        Some(format!(
                            "file:///tmp/{}/metadata/00000.json",
                            table_ident.name
                        )),
                        serde_json::json!({"format-version": 3}),
                        Principal::anonymous(),
                    ))
                    .await
                    .unwrap();
            }
            store
                .commit_table(
                    &ident,
                    TableCommit {
                        requirements: vec![],
                        updates: vec![serde_json::json!({"action": "noop"})],
                        expected_previous_metadata_location: Some(
                            "file:///tmp/events/metadata/00000.json".to_string(),
                        ),
                        new_metadata_location: Some(
                            "file:///tmp/events/metadata/00001.json".to_string(),
                        ),
                        new_metadata: Some(serde_json::json!({"format-version": 3})),
                        idempotency_key: None,
                        idempotency_request_hash: None,
                        principal: Principal::anonymous(),
                        authorization_receipt: None,
                    },
                )
                .await
                .unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update metadata_pointer_log set table_key = ?2 where table_key = ?1",
                (table_key(&ident), table_key(&other_ident)),
            )
            .await
            .unwrap();

            assert_eq!(
                store.table_commit_records(&ident, 0, None).await.unwrap(),
                vec![]
            );
            let err = store
                .table_commit_records(&other_ident, 0, None)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("table commit record table does not match requested table")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_commit_history_principal_row_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            let writer =
                Principal::new("did:example:writer", lakecat_core::PrincipalKind::Agent).unwrap();
            let shadow =
                Principal::new("did:example:shadow", lakecat_core::PrincipalKind::Agent).unwrap();
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    writer.clone(),
                ))
                .await
                .unwrap();
            store
                .commit_table(
                    &ident,
                    TableCommit {
                        requirements: vec![],
                        updates: vec![serde_json::json!({"action": "noop"})],
                        expected_previous_metadata_location: Some(
                            "file:///tmp/events/metadata/00000.json".to_string(),
                        ),
                        new_metadata_location: Some(
                            "file:///tmp/events/metadata/00001.json".to_string(),
                        ),
                        new_metadata: Some(serde_json::json!({"format-version": 3})),
                        idempotency_key: None,
                        idempotency_request_hash: None,
                        principal: writer,
                        authorization_receipt: None,
                    },
                )
                .await
                .unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update metadata_pointer_log set principal_json = ?2 where table_key = ?1",
                (table_key(&ident), encode_json(&shadow).unwrap()),
            )
            .await
            .unwrap();

            let err = store
                .table_commit_records(&ident, 0, None)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains(
                        "table commit record principal does not match pointer log row"
                    )
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_table_record_json_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                ))
                .await
                .unwrap();
            let mut table = store.load_table(&ident).await.unwrap();
            table.ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("other_events").unwrap(),
            );
            let conn = store.connect().unwrap();
            conn.execute(
                "update tables set record_json = ?2 where table_key = ?1",
                (table_key(&ident), encode_json(&table).unwrap()),
            )
            .await
            .unwrap();

            let err = store.load_table(&ident).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("table record row scope does not match")
            ));
            let err = store.list_tables(&warehouse).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("table record row scope does not match")
            ));
            let err = store
                .commit_table(
                    &ident,
                    TableCommit {
                        requirements: vec![],
                        updates: vec![serde_json::json!({"action": "noop"})],
                        expected_previous_metadata_location: Some(
                            "file:///tmp/events/metadata/00000.json".to_string(),
                        ),
                        new_metadata_location: Some(
                            "file:///tmp/events/metadata/00001.json".to_string(),
                        ),
                        new_metadata: Some(serde_json::json!({"format-version": 3})),
                        idempotency_key: None,
                        idempotency_request_hash: None,
                        principal: Principal::anonymous(),
                        authorization_receipt: None,
                    },
                )
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("table record row scope does not match")
            ));
            let err = store
                .soft_delete_table(&ident, Principal::anonymous(), None)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("table record row scope does not match")
            ));
            assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
            assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 0);
        }

        #[tokio::test]
        async fn turso_store_rejects_table_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                ))
                .await
                .unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update tables
                 set namespace_path = ?2, table_name = ?3
                 where table_key = ?1",
                (table_key(&ident), "other", "other_events"),
            )
            .await
            .unwrap();

            for err in [
                store.load_table(&ident).await.unwrap_err(),
                store.list_tables(&warehouse).await.unwrap_err(),
                store
                    .commit_table(
                        &ident,
                        TableCommit {
                            requirements: vec![],
                            updates: vec![serde_json::json!({"action": "noop"})],
                            expected_previous_metadata_location: Some(
                                "file:///tmp/events/metadata/00000.json".to_string(),
                            ),
                            new_metadata_location: Some(
                                "file:///tmp/events/metadata/00001.json".to_string(),
                            ),
                            new_metadata: Some(serde_json::json!({"format-version": 3})),
                            idempotency_key: None,
                            idempotency_request_hash: None,
                            principal: Principal::anonymous(),
                            authorization_receipt: None,
                        },
                    )
                    .await
                    .unwrap_err(),
                store
                    .soft_delete_table(&ident, Principal::anonymous(), None)
                    .await
                    .unwrap_err(),
            ] {
                assert!(matches!(
                    err,
                    LakeCatError::Internal(message)
                        if message.contains("table record row scope does not match")
                ));
            }
            assert_eq!(store.count_rows("metadata_pointer_log").await.unwrap(), 0);
            assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 0);

            let restore_store = TursoCatalogStore::in_memory().await.unwrap();
            restore_store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            restore_store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                ))
                .await
                .unwrap();
            restore_store
                .soft_delete_table(&ident, Principal::anonymous(), None)
                .await
                .unwrap();
            let conn = restore_store.connect().unwrap();
            conn.execute(
                "update tables
                 set namespace_path = ?2, table_name = ?3
                 where table_key = ?1",
                (table_key(&ident), "other", "other_events"),
            )
            .await
            .unwrap();

            let err = restore_store
                .restore_table(&ident, Principal::anonymous(), None)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("table record row scope does not match")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_table_idempotency_response_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                ))
                .await
                .unwrap();
            store
                .commit_table(
                    &ident,
                    TableCommit {
                        requirements: vec![],
                        updates: vec![serde_json::json!({"action": "noop"})],
                        expected_previous_metadata_location: Some(
                            "file:///tmp/events/metadata/00000.json".to_string(),
                        ),
                        new_metadata_location: Some(
                            "file:///tmp/events/metadata/00001.json".to_string(),
                        ),
                        new_metadata: Some(serde_json::json!({"format-version": 3})),
                        idempotency_key: Some("commit-1".to_string()),
                        idempotency_request_hash: None,
                        principal: Principal::anonymous(),
                        authorization_receipt: None,
                    },
                )
                .await
                .unwrap();
            let record = store
                .table_commit_records(&ident, 1, Some(1))
                .await
                .unwrap()
                .pop()
                .unwrap();
            let mut response = store.load_table(&ident).await.unwrap();
            response.ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("other_events").unwrap(),
            );
            let conn = store.connect().unwrap();
            conn.execute(
                "update idempotency_records set response_json = ?2 where idem_key = ?1",
                (
                    idempotency_record_key(&ident, "commit-1"),
                    encode_json(&response).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store
                .replay_table_commit(&ident, "commit-1", &record.request_hash)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("table record row scope does not match")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_table_idempotency_row_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .unwrap();
            store
                .create_table(TableRecord::new(
                    ident.clone(),
                    "file:///tmp/events".to_string(),
                    Some("file:///tmp/events/metadata/00000.json".to_string()),
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                ))
                .await
                .unwrap();
            let commit = TableCommit {
                requirements: vec![],
                updates: vec![serde_json::json!({"action": "noop"})],
                expected_previous_metadata_location: Some(
                    "file:///tmp/events/metadata/00000.json".to_string(),
                ),
                new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
                new_metadata: Some(serde_json::json!({"format-version": 3})),
                idempotency_key: Some("commit-1".to_string()),
                idempotency_request_hash: None,
                principal: Principal::anonymous(),
                authorization_receipt: None,
            };
            store.commit_table(&ident, commit.clone()).await.unwrap();
            let record = store
                .table_commit_records(&ident, 1, Some(1))
                .await
                .unwrap()
                .pop()
                .unwrap();
            let other_ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("other_events").unwrap(),
            );
            let conn = store.connect().unwrap();
            conn.execute(
                "update idempotency_records set table_key = ?2 where idem_key = ?1",
                (
                    idempotency_record_key(&ident, "commit-1"),
                    crate::table_key(&other_ident),
                ),
            )
            .await
            .unwrap();

            let err = store
                .replay_table_commit(&ident, "commit-1", &record.request_hash)
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("idempotency record row scope does not match")
            ));
            let err = store.commit_table(&ident, commit).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("idempotency record row scope does not match")
            ));
        }

        #[tokio::test]
        async fn turso_store_orders_pending_outbox_events_deterministically() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let mut events = Vec::new();
            for event_type in ["querygraph.bootstrap.b", "querygraph.bootstrap.a"] {
                let mut event = CatalogAuditEvent::new(
                    event_type,
                    Some(ident.clone()),
                    Principal::anonymous(),
                    serde_json::json!({
                        "event-type": event_type,
                        "table": ident.clone(),
                        "sequence": event_type,
                    }),
                )
                .unwrap();
                event.created_at = "2026-01-01T00:00:00Z".parse().unwrap();
                let audit_event_id = crate::audit_event_id(&event).unwrap();
                let outbox_payload = crate::audit_outbox_payload(&audit_event_id, &event);
                let outbox_event =
                    crate::outbox_event_from_payload(&outbox_payload, event.created_at)
                        .expect("test event should produce an outbox event");
                events.push((outbox_event.event_id, event));
            }
            events.sort_by(|left, right| right.0.cmp(&left.0));
            for (_, event) in events {
                store.record_audit_event(event).await.unwrap();
            }

            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            let event_ids = pending
                .iter()
                .map(|event| event.event_id.clone())
                .collect::<Vec<_>>();
            let mut sorted_event_ids = event_ids.clone();
            sorted_event_ids.sort();
            assert_eq!(event_ids, sorted_event_ids);
            assert_eq!(
                store
                    .mark_outbox_delivered(&[event_ids[0].clone(), event_ids[0].clone()])
                    .await
                    .unwrap(),
                1
            );
        }

        #[tokio::test]
        async fn turso_store_omits_table_from_unscoped_audit_outbox_events() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            store
                .record_audit_event(
                    CatalogAuditEvent::new(
                        "catalog.config-read",
                        None,
                        Principal::anonymous(),
                        serde_json::json!({
                            "event-type": "catalog.config-read",
                            "authorization-receipt": {
                                "engine": "typesec",
                                "allowed": true,
                                "action": "catalog-config"
                            },
                            "warehouse": "local"
                        }),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();

            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
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
                            "version": 1
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
            let config = pending
                .iter()
                .find(|event| event.event_type == "catalog.config-read")
                .expect("config-read event");
            assert!(
                config.payload.get("table").is_none(),
                "unscoped config-read wrapper must not carry table evidence"
            );
            let table = pending
                .iter()
                .find(|event| event.event_type == "table.loaded")
                .expect("table-loaded event");
            assert!(
                table.payload.get("table").is_some(),
                "table-scoped wrapper must preserve table evidence"
            );
        }

        #[tokio::test]
        async fn turso_store_rejects_malformed_outbox_delivery_ids() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
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
                            "manifest-hash": "lakecat:test"
                        }),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();

            let err = store
                .mark_outbox_delivered(&["sha256:short".to_string()])
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("outbox event id must be full SHA-256 evidence")
            ));
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 1);
            assert!(pending[0].delivered_at.is_none());
        }

        #[tokio::test]
        async fn turso_store_rolls_back_audit_when_outbox_insert_fails() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let mut event = CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap();
            event.created_at = "2026-01-01T00:00:00Z".parse().unwrap();
            let audit_event_id = crate::audit_event_id(&event).unwrap();
            let outbox_payload = crate::audit_outbox_payload(&audit_event_id, &event);
            let outbox_event = crate::outbox_event_from_payload(&outbox_payload, event.created_at)
                .expect("test event should produce an outbox event");

            let conn = store.connect().unwrap();
            conn.execute(
                "insert into outbox_events (
                    event_id, sink, event_type, payload_json, created_at
                 )
                 values (?1, ?2, ?3, ?4, ?5)",
                (
                    outbox_event.event_id.as_str(),
                    outbox_event.sink.as_str(),
                    outbox_event.event_type.as_str(),
                    encode_json(&outbox_event.payload).unwrap(),
                    outbox_event.created_at.to_rfc3339(),
                ),
            )
            .await
            .unwrap();

            let err = store.record_audit_event(event).await.unwrap_err();
            assert!(
                matches!(&err, LakeCatError::Internal(message) if message.contains("UNIQUE") || message.contains("PRIMARY KEY")),
                "unexpected error: {err:?}"
            );
            assert_eq!(
                store.count_rows("audit_events").await.unwrap(),
                0,
                "audit insert must roll back when transactional outbox insert fails"
            );
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 1);
        }

        #[tokio::test]
        async fn turso_store_duplicate_audit_write_does_not_duplicate_outbox() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let mut event = CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap();
            event.created_at = "2026-01-01T00:00:00Z".parse().unwrap();

            store.record_audit_event(event.clone()).await.unwrap();
            let err = store.record_audit_event(event).await.unwrap_err();
            assert!(
                matches!(&err, LakeCatError::Internal(message) if message.contains("UNIQUE") || message.contains("PRIMARY KEY")),
                "unexpected error: {err:?}"
            );
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 1);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 1);
            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].event_type, "querygraph.bootstrap");
        }

        #[tokio::test]
        async fn turso_store_rejects_audit_event_type_drift_before_outbox() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let mut event = CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap();
            event.event_type = "querygraph.bootstrap.drifted".to_string();

            let err = store.record_audit_event(event).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("audit event type does not match payload")
            ));
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
        }

        #[tokio::test]
        async fn turso_store_rejects_audit_events_without_request_hash() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let mut event = CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident.clone()),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap();
            event.request_hash = None;

            let err = store.record_audit_event(event).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("audit event request hash is required")
            ));
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
        }

        #[tokio::test]
        async fn turso_store_rejects_audit_payload_table_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let other_ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("other_events").unwrap(),
            );
            let event = CatalogAuditEvent::new(
                "querygraph.bootstrap",
                Some(ident),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "table": other_ident,
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap();

            let err = store.record_audit_event(event).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("audit event payload table scope does not match")
            ));
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
        }

        #[tokio::test]
        async fn turso_store_rejects_bare_table_name_audit_payload_scope() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let event = CatalogAuditEvent::new(
                "table.commits-listed",
                Some(ident),
                Principal::anonymous(),
                serde_json::json!({
                    "event-type": "table.commits-listed",
                    "table": "events",
                    "commit-count": 0
                }),
            )
            .unwrap();

            let err = store.record_audit_event(event).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("audit event payload missing warehouse scope for table")
            ));
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
        }

        #[tokio::test]
        async fn turso_store_rejects_audit_authorization_principal_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let event_principal =
                Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent).unwrap();
            let receipt_principal =
                Principal::new("human:operator", lakecat_core::PrincipalKind::Human).unwrap();
            let event = CatalogAuditEvent::new(
                "querygraph.bootstrap",
                None,
                event_principal,
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "principal": receipt_principal,
                        "action": "querygraph.bootstrap"
                    },
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap();

            let err = store.record_audit_event(event).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains(
                        "audit event authorization receipt principal does not match event principal"
                    )
            ));
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
        }

        #[tokio::test]
        async fn turso_store_rejects_audit_authorization_receipts_without_action() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let principal =
                Principal::new("did:example:agent", lakecat_core::PrincipalKind::Agent).unwrap();
            let event = CatalogAuditEvent::new(
                "querygraph.bootstrap",
                None,
                principal.clone(),
                serde_json::json!({
                    "event-type": "querygraph.bootstrap",
                    "authorization-receipt": {
                        "engine": "typesec",
                        "allowed": true,
                        "principal": principal
                    },
                    "manifest-hash": "lakecat:test"
                }),
            )
            .unwrap();

            let err = store.record_audit_event(event).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("audit event authorization receipt action is required")
            ));
            assert_eq!(store.count_rows("audit_events").await.unwrap(), 0);
            assert_eq!(store.count_rows("outbox_events").await.unwrap(), 0);
        }

        #[tokio::test]
        async fn turso_store_limits_pending_outbox_after_deterministic_ordering() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let ident = TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            );
            let mut expected = Vec::new();
            let mut events = Vec::new();
            for (event_type, created_at) in [
                ("querygraph.bootstrap.late", "2026-01-01T00:00:02Z"),
                ("querygraph.bootstrap.tie-b", "2026-01-01T00:00:01Z"),
                ("querygraph.bootstrap.tie-a", "2026-01-01T00:00:01Z"),
            ] {
                let mut event = CatalogAuditEvent::new(
                    event_type,
                    Some(ident.clone()),
                    Principal::anonymous(),
                    serde_json::json!({
                        "event-type": event_type,
                        "table": ident.clone(),
                        "sequence": event_type,
                    }),
                )
                .unwrap();
                event.created_at = created_at.parse().unwrap();
                let audit_event_id = crate::audit_event_id(&event).unwrap();
                let outbox_payload = crate::audit_outbox_payload(&audit_event_id, &event);
                let outbox_event =
                    crate::outbox_event_from_payload(&outbox_payload, event.created_at)
                        .expect("test event should produce an outbox event");
                expected.push((outbox_event.created_at, outbox_event.event_id.clone()));
                events.push((outbox_event.event_id, event));
            }
            events.sort_by(|left, right| right.0.cmp(&left.0));
            for (_, event) in events {
                store.record_audit_event(event).await.unwrap();
            }
            expected.sort();

            let pending = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 2)
                .await
                .unwrap();
            let event_ids = pending
                .iter()
                .map(|event| event.event_id.clone())
                .collect::<Vec<_>>();
            assert_eq!(
                event_ids,
                expected
                    .iter()
                    .take(2)
                    .map(|(_, event_id)| event_id.clone())
                    .collect::<Vec<_>>()
            );
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

            let LakeCatError::Conflict(message) = err else {
                panic!("stale metadata pointer must return conflict");
            };
            assert!(message.contains("expected-metadata-location-hash=sha256:"));
            assert!(message.contains("actual-metadata-location-hash=sha256:"));
            assert!(!message.contains("stale.json"));
            assert!(!message.contains("00000.json"));
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
        async fn turso_store_rejects_corrupt_pending_outbox_payloads() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
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

            let mut drifted_payload = pending[0].payload.clone();
            drifted_payload["event-type"] = serde_json::json!("querygraph.bootstrap.drifted");
            let conn = store.connect().unwrap();
            conn.execute(
                "update outbox_events set payload_json = ?2 where event_id = ?1",
                (
                    pending[0].event_id.as_str(),
                    encode_json(&drifted_payload).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap_err();
            let LakeCatError::Internal(message) = err else {
                panic!("expected internal pending outbox validation error");
            };
            assert!(
                message.contains("pending outbox event type does not match payload")
                    || message.contains("pending outbox event id does not match payload hash"),
                "{message}"
            );
            assert!(message.contains("event-id-hash=sha256:"), "{message}");
            assert!(message.contains("event-type-hash=sha256:"), "{message}");
            assert!(message.contains("payload-hash=sha256:"), "{message}");
            assert!(
                !message.contains("querygraph.bootstrap.drifted"),
                "{message}"
            );
        }

        #[tokio::test]
        async fn turso_store_rejects_blank_pending_outbox_event_types() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
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

            let blank_payload = serde_json::json!({
                "event-type": " ",
                "manifest-hash": "lakecat:test"
            });
            let blank_payload_hash = crate::content_hash_json(&blank_payload).unwrap();
            let conn = store.connect().unwrap();
            conn.execute(
                "update outbox_events
                 set event_id = ?2, event_type = ?3, payload_json = ?4
                 where event_id = ?1",
                (
                    pending[0].event_id.as_str(),
                    blank_payload_hash.as_str(),
                    " ",
                    encode_json(&blank_payload).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap_err();
            let LakeCatError::Internal(message) = err else {
                panic!("expected internal pending outbox validation error");
            };
            assert!(message.contains("pending outbox event type must not be empty"));
            assert!(message.contains("event-id-hash=sha256:"), "{message}");
            assert!(message.contains("event-type-hash=sha256:"), "{message}");
            assert!(message.contains("payload-hash=sha256:"), "{message}");
            assert!(!message.contains("manifest-hash"), "{message}");
            assert!(!message.contains("lakecat:test"), "{message}");
        }

        #[tokio::test]
        async fn turso_store_rejects_blank_pending_outbox_payload_event_types() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
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

            let blank_payload = serde_json::json!({
                "event-type": " ",
                "manifest-hash": "lakecat:test"
            });
            let blank_payload_hash = crate::content_hash_json(&blank_payload).unwrap();
            let conn = store.connect().unwrap();
            conn.execute(
                "update outbox_events
                 set event_id = ?2, payload_json = ?3
                 where event_id = ?1",
                (
                    pending[0].event_id.as_str(),
                    blank_payload_hash.as_str(),
                    encode_json(&blank_payload).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store
                .pending_outbox_events(Some("lakecat.lineage-and-graph"), 10)
                .await
                .unwrap_err();
            let LakeCatError::Internal(message) = err else {
                panic!("expected internal pending outbox validation error");
            };
            assert!(message.contains("pending outbox payload event-type must not be empty"));
            assert!(message.contains("event-id-hash=sha256:"), "{message}");
            assert!(message.contains("event-type-hash=sha256:"), "{message}");
            assert!(
                message.contains("payload-event-type-hash=sha256:"),
                "{message}"
            );
            assert!(message.contains("payload-hash=sha256:"), "{message}");
            assert!(!message.contains("manifest-hash"), "{message}");
            assert!(!message.contains("lakecat:test"), "{message}");
        }

        #[tokio::test]
        async fn turso_store_rejects_blank_pending_outbox_sinks() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
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

            let conn = store.connect().unwrap();
            conn.execute(
                "update outbox_events set sink = ?2 where event_id = ?1",
                (pending[0].event_id.as_str(), " "),
            )
            .await
            .unwrap();

            let err = store.pending_outbox_events(None, 10).await.unwrap_err();
            let LakeCatError::Internal(message) = err else {
                panic!("expected internal pending outbox validation error");
            };
            assert!(message.contains("pending outbox event sink must not be empty"));
            assert!(message.contains("event-id-hash=sha256:"), "{message}");
            assert!(message.contains("event-type-hash=sha256:"), "{message}");
            assert!(message.contains("payload-hash=sha256:"), "{message}");
            assert!(!message.contains("manifest-hash"), "{message}");
            assert!(!message.contains("lakecat:test"), "{message}");
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
            let conflict_message = results
                .iter()
                .find_map(|result| match result {
                    Err(LakeCatError::Conflict(message)) => Some(message.as_str()),
                    _ => None,
                })
                .expect("one concurrent commit should conflict");

            assert_eq!(success_count, 1);
            assert_eq!(conflict_count, 1);
            assert!(conflict_message.contains("expected-metadata-location-hash=sha256:"));
            assert!(conflict_message.contains("actual-metadata-location-hash=sha256:"));
            assert!(!conflict_message.contains("00000.json"));
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
        async fn turso_store_rejects_corrupt_storage_profiles_on_read() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let table = TableRecord::new(
                TableIdent::new(
                    warehouse.clone(),
                    "default".parse::<Namespace>().unwrap(),
                    TableName::new("events").unwrap(),
                ),
                "s3://lakecat-demo/events/table".to_string(),
                None,
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            let mut profile = StorageProfile::new(
                "s3-events",
                warehouse.clone(),
                "s3://lakecat-demo/events",
                StorageProvider::S3,
                CredentialIssuanceMode::ShortLivedSecretRef,
                Some("typesec://lakecat/local/s3-events".to_string()),
                BTreeMap::new(),
            )
            .unwrap();
            store.upsert_storage_profile(profile.clone()).await.unwrap();
            profile.profile_id = "s3-events?token=secret".to_string();

            let conn = store.connect().unwrap();
            conn.execute(
                "update storage_profiles set profile_json = ?2 where profile_key = ?1",
                (
                    storage_profile_key(&warehouse, "s3-events"),
                    encode_json(&profile).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store.list_storage_profiles(&warehouse).await.unwrap_err();
            let message = err.to_string();
            assert!(message.contains("storage-profile-id-hash=sha256:"));
            assert!(!message.contains("token=secret"));

            let err = store.storage_profile_for_table(&table).await.unwrap_err();
            let message = err.to_string();
            assert!(message.contains("storage-profile-id-hash=sha256:"));
            assert!(!message.contains("token=secret"));
        }

        #[tokio::test]
        async fn turso_store_rejects_storage_profile_json_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let table = TableRecord::new(
                TableIdent::new(
                    warehouse.clone(),
                    "default".parse::<Namespace>().unwrap(),
                    TableName::new("events").unwrap(),
                ),
                "s3://lakecat-demo/events/table".to_string(),
                None,
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            let mut profile = StorageProfile::new(
                "s3-events",
                warehouse.clone(),
                "s3://lakecat-demo/events",
                StorageProvider::S3,
                CredentialIssuanceMode::ShortLivedSecretRef,
                Some("typesec://lakecat/local/s3-events".to_string()),
                BTreeMap::new(),
            )
            .unwrap();
            store.upsert_storage_profile(profile.clone()).await.unwrap();
            profile.profile_id = "other-profile".to_string();

            let conn = store.connect().unwrap();
            conn.execute(
                "update storage_profiles set profile_json = ?2 where profile_key = ?1",
                (
                    storage_profile_key(&warehouse, "s3-events"),
                    encode_json(&profile).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store.list_storage_profiles(&warehouse).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("storage profile row scope does not match")
            ));

            let err = store.storage_profile_for_table(&table).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("storage profile row scope does not match")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_storage_profile_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let table = TableRecord::new(
                TableIdent::new(
                    warehouse.clone(),
                    "default".parse::<Namespace>().unwrap(),
                    TableName::new("events").unwrap(),
                ),
                "s3://lakecat-demo/events/table".to_string(),
                None,
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            let profile = StorageProfile::new(
                "s3-events",
                warehouse.clone(),
                "s3://lakecat-demo/events",
                StorageProvider::S3,
                CredentialIssuanceMode::ShortLivedSecretRef,
                Some("typesec://lakecat/local/s3-events".to_string()),
                BTreeMap::new(),
            )
            .unwrap();
            store.upsert_storage_profile(profile).await.unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update storage_profiles
                 set location_prefix = ?2, provider = ?3, issuance_mode = ?4
                 where profile_key = ?1",
                (
                    storage_profile_key(&warehouse, "s3-events"),
                    "s3://lakecat-demo/other",
                    "gcs",
                    "governed-read-required",
                ),
            )
            .await
            .unwrap();

            let err = store.list_storage_profiles(&warehouse).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("storage profile row scope does not match")
            ));

            let err = store.storage_profile_for_table(&table).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("storage profile row scope does not match")
            ));
        }

        #[tokio::test]
        async fn storage_profile_matching_rejects_ambiguous_same_prefix_profiles() {
            let store = MemoryCatalogStore::new();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let table = TableRecord::new(
                TableIdent::new(
                    warehouse.clone(),
                    namespace,
                    TableName::new("events").unwrap(),
                ),
                "s3://lakecat-demo/events/tenant-a/table".to_string(),
                None,
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            for profile_id in ["events-a", "events-b"] {
                store
                    .upsert_storage_profile(
                        StorageProfile::new(
                            profile_id,
                            warehouse.clone(),
                            "s3://lakecat-demo/events",
                            StorageProvider::S3,
                            CredentialIssuanceMode::GovernedReadRequired,
                            None,
                            BTreeMap::new(),
                        )
                        .unwrap(),
                    )
                    .await
                    .unwrap();
            }

            let err = store.storage_profile_for_table(&table).await.unwrap_err();
            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("ambiguous storage profile match"));
            assert!(message.contains("location-prefix-hash=sha256:"));
            assert!(message.contains("events-a"));
            assert!(message.contains("events-b"));
            assert!(!message.contains("s3://lakecat-demo/events"));
        }

        #[tokio::test]
        async fn turso_storage_profile_matching_rejects_ambiguous_same_prefix_profiles() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let table = TableRecord::new(
                TableIdent::new(
                    warehouse.clone(),
                    namespace,
                    TableName::new("events").unwrap(),
                ),
                "s3://lakecat-demo/events/tenant-a/table".to_string(),
                None,
                serde_json::json!({"format-version": 3}),
                Principal::anonymous(),
            );
            for profile_id in ["events-a", "events-b"] {
                store
                    .upsert_storage_profile(
                        StorageProfile::new(
                            profile_id,
                            warehouse.clone(),
                            "s3://lakecat-demo/events",
                            StorageProvider::S3,
                            CredentialIssuanceMode::GovernedReadRequired,
                            None,
                            BTreeMap::new(),
                        )
                        .unwrap(),
                    )
                    .await
                    .unwrap();
            }

            let err = store.storage_profile_for_table(&table).await.unwrap_err();
            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("ambiguous storage profile match"));
            assert!(message.contains("location-prefix-hash=sha256:"));
            assert!(message.contains("events-a"));
            assert!(message.contains("events-b"));
            assert!(!message.contains("s3://lakecat-demo/events"));
        }

        #[tokio::test]
        async fn storage_profile_matching_respects_location_boundaries() {
            let store = MemoryCatalogStore::new();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let profile = StorageProfile::new(
                "events-root",
                warehouse.clone(),
                "s3://lakecat-demo/events",
                StorageProvider::S3,
                CredentialIssuanceMode::GovernedReadRequired,
                None,
                BTreeMap::from([("lakecat.scope".to_string(), "events".to_string())]),
            )
            .unwrap();
            store.upsert_storage_profile(profile.clone()).await.unwrap();

            for (table_name, location, expected_profile_id) in [
                ("events-exact", "s3://lakecat-demo/events", "events-root"),
                (
                    "events-child",
                    "s3://lakecat-demo/events/tenant-a/table",
                    "events-root",
                ),
                (
                    "events-sibling",
                    "s3://lakecat-demo/events-shadow/table",
                    "local:s3",
                ),
            ] {
                let table = TableRecord::new(
                    TableIdent::new(
                        warehouse.clone(),
                        namespace.clone(),
                        TableName::new(table_name).unwrap(),
                    ),
                    location.to_string(),
                    None,
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                );

                let matched = store.storage_profile_for_table(&table).await.unwrap();
                assert_eq!(matched.profile_id, expected_profile_id, "{location}");
            }
        }

        #[tokio::test]
        async fn turso_storage_profile_matching_respects_trailing_slash_boundaries() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let profile = StorageProfile::new(
                "events-root",
                warehouse.clone(),
                "s3://lakecat-demo/events/",
                StorageProvider::S3,
                CredentialIssuanceMode::GovernedReadRequired,
                None,
                BTreeMap::from([("lakecat.scope".to_string(), "events".to_string())]),
            )
            .unwrap();
            store.upsert_storage_profile(profile.clone()).await.unwrap();

            for (table_name, location, expected_profile_id) in [
                (
                    "events-child",
                    "s3://lakecat-demo/events/tenant-a/table",
                    "events-root",
                ),
                (
                    "events-sibling",
                    "s3://lakecat-demo/events-shadow/table",
                    "local:s3",
                ),
            ] {
                let table = TableRecord::new(
                    TableIdent::new(
                        warehouse.clone(),
                        namespace.clone(),
                        TableName::new(table_name).unwrap(),
                    ),
                    location.to_string(),
                    None,
                    serde_json::json!({"format-version": 3}),
                    Principal::anonymous(),
                );

                let matched = store.storage_profile_for_table(&table).await.unwrap();
                assert_eq!(matched.profile_id, expected_profile_id, "{location}");
            }
        }

        #[test]
        fn storage_profiles_reject_dot_segment_location_prefixes() {
            let warehouse = WarehouseName::new("local").unwrap();
            for location_prefix in [
                "s3://lakecat-demo/events/../private",
                "s3://lakecat-demo/events/%2e%2e/private",
                "file:///tmp/lakecat/%2E/events",
            ] {
                let err = StorageProfile::new(
                    "dot-prefix",
                    warehouse.clone(),
                    location_prefix,
                    StorageProvider::from_location(location_prefix),
                    CredentialIssuanceMode::GovernedReadRequired,
                    None,
                    BTreeMap::new(),
                )
                .unwrap_err();

                let message = err.to_string();
                assert!(matches!(err, LakeCatError::InvalidArgument(_)));
                assert!(message.contains("dot path segments"));
                assert!(message.contains("storage-profile-prefix-hash=sha256:"));
                assert!(
                    !message.contains(location_prefix),
                    "dot-segment location-prefix validation must not expose raw storage roots"
                );
            }
        }

        #[test]
        fn location_prefix_dot_segment_detection_allows_ordinary_dotted_names() {
            assert!(crate::location_prefix_has_dot_path_segment(
                "s3://lakecat-demo/events/../private"
            ));
            assert!(crate::location_prefix_has_dot_path_segment(
                "s3://lakecat-demo/events/%2e%2e/private"
            ));
            assert!(!crate::location_prefix_has_dot_path_segment(
                "s3://lakecat-demo/events/service.v1/table"
            ));
        }

        #[test]
        fn storage_profiles_reject_decorated_location_prefixes() {
            let warehouse = WarehouseName::new("local").unwrap();
            for (location_prefix, provider) in [
                ("s3://lakecat-demo/events?token=abc", StorageProvider::S3),
                ("s3://lakecat-demo/events#current", StorageProvider::S3),
                ("s3://user:secret@lakecat-demo/events", StorageProvider::S3),
                (
                    "file:///tmp/lakecat/events?debug=true",
                    StorageProvider::File,
                ),
            ] {
                let err = StorageProfile::new(
                    "decorated-prefix",
                    warehouse.clone(),
                    location_prefix,
                    provider,
                    CredentialIssuanceMode::GovernedReadRequired,
                    None,
                    BTreeMap::new(),
                )
                .unwrap_err();

                let message = err.to_string();
                assert!(matches!(err, LakeCatError::InvalidArgument(_)));
                assert!(message.contains("query strings, fragments, or userinfo"));
                assert!(message.contains("storage-profile-prefix-hash=sha256:"));
                assert!(
                    !message.contains(location_prefix),
                    "decorated location-prefix validation must not expose raw storage roots"
                );
                assert!(!message.contains("token=abc"));
                assert!(!message.contains("user:secret"));
            }
        }

        #[tokio::test]
        async fn storage_profile_upsert_rejects_deserialized_decorated_location_prefixes() {
            let warehouse = WarehouseName::new("local").unwrap();
            let profile = StorageProfile {
                profile_id: "decorated-prefix".to_string(),
                warehouse: warehouse.clone(),
                location_prefix: "s3://lakecat-demo/events?token=abc".to_string(),
                provider: StorageProvider::S3,
                issuance_mode: CredentialIssuanceMode::GovernedReadRequired,
                secret_ref: None,
                public_config: BTreeMap::new(),
            };

            let memory_err = MemoryCatalogStore::new()
                .upsert_storage_profile(profile.clone())
                .await
                .unwrap_err();
            let message = memory_err.to_string();
            assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
            assert!(message.contains("query strings, fragments, or userinfo"));
            assert!(message.contains("storage-profile-prefix-hash=sha256:"));
            assert!(!message.contains("s3://lakecat-demo/events?token=abc"));
            assert!(!message.contains("token=abc"));

            let turso = TursoCatalogStore::in_memory().await.unwrap();
            let turso_err = turso.upsert_storage_profile(profile).await.unwrap_err();
            let message = turso_err.to_string();
            assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
            assert!(message.contains("query strings, fragments, or userinfo"));
            assert!(message.contains("storage-profile-prefix-hash=sha256:"));
            assert!(!message.contains("s3://lakecat-demo/events?token=abc"));
            assert!(!message.contains("token=abc"));
            assert_eq!(
                turso.list_storage_profiles(&warehouse).await.unwrap(),
                vec![]
            );
        }

        #[tokio::test]
        async fn storage_profile_upsert_rejects_deserialized_dot_segment_location_prefixes() {
            let warehouse = WarehouseName::new("local").unwrap();
            let profile = StorageProfile {
                profile_id: "dot-prefix".to_string(),
                warehouse: warehouse.clone(),
                location_prefix: "s3://lakecat-demo/events/../private".to_string(),
                provider: StorageProvider::S3,
                issuance_mode: CredentialIssuanceMode::GovernedReadRequired,
                secret_ref: None,
                public_config: BTreeMap::new(),
            };

            let memory_err = MemoryCatalogStore::new()
                .upsert_storage_profile(profile.clone())
                .await
                .unwrap_err();
            assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
            assert!(memory_err.to_string().contains("dot path segments"));
            assert!(
                memory_err
                    .to_string()
                    .contains("storage-profile-prefix-hash=sha256:")
            );

            let turso = TursoCatalogStore::in_memory().await.unwrap();
            let turso_err = turso.upsert_storage_profile(profile).await.unwrap_err();
            assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
            assert!(turso_err.to_string().contains("dot path segments"));
            assert!(
                turso_err
                    .to_string()
                    .contains("storage-profile-prefix-hash=sha256:")
            );
            assert_eq!(
                turso.list_storage_profiles(&warehouse).await.unwrap(),
                vec![]
            );
        }

        #[test]
        fn warehouses_reject_decorated_storage_roots() {
            let warehouse = WarehouseName::new("local").unwrap();
            for storage_root in [
                "file:///tmp/lakecat?token=abc",
                "s3://lakecat-demo/root#current",
                "s3://user:secret@lakecat-demo/root",
            ] {
                let err = WarehouseRecord::new(
                    warehouse.clone(),
                    "default",
                    Some(storage_root.to_string()),
                    BTreeMap::new(),
                    Principal::anonymous(),
                )
                .unwrap_err();

                let message = err.to_string();
                assert!(matches!(err, LakeCatError::InvalidArgument(_)));
                assert!(message.contains("query strings, fragments, or userinfo"));
                assert!(message.contains("warehouse-storage-root-hash=sha256:"));
                assert!(
                    !message.contains(storage_root),
                    "warehouse storage-root validation must not expose raw storage roots"
                );
                assert!(!message.contains("token=abc"));
                assert!(!message.contains("user:secret"));
            }
        }

        #[test]
        fn servers_reject_decorated_endpoint_urls() {
            for endpoint_url in [
                "https://lakecat.example.com?token=abc",
                "https://lakecat.example.com/catalog#frag",
                "https://user:secret@lakecat.example.com/catalog",
            ] {
                let err = ServerRecord::new(
                    "prod",
                    Some("Production".to_string()),
                    Some(endpoint_url.to_string()),
                    BTreeMap::new(),
                    Principal::anonymous(),
                )
                .unwrap_err();

                let message = err.to_string();
                assert!(matches!(err, LakeCatError::InvalidArgument(_)));
                assert!(message.contains("query strings, fragments, or userinfo"));
                assert!(message.contains("server-endpoint-url-hash=sha256:"));
                assert!(
                    !message.contains(endpoint_url),
                    "server endpoint validation must not expose raw endpoint URLs"
                );
                assert!(!message.contains("token=abc"));
                assert!(!message.contains("user:secret"));
            }
        }

        #[test]
        fn servers_reject_invalid_endpoint_urls() {
            for (endpoint_url, expected) in [
                ("lakecat.example.com/catalog", "absolute http or https URL"),
                ("not a url", "absolute http or https URL"),
                ("file:///tmp/lakecat", "http or https scheme"),
                ("s3://lakecat-demo/catalog", "http or https scheme"),
            ] {
                let err = ServerRecord::new(
                    "prod",
                    Some("Production".to_string()),
                    Some(endpoint_url.to_string()),
                    BTreeMap::new(),
                    Principal::anonymous(),
                )
                .unwrap_err();

                let message = err.to_string();
                assert!(matches!(err, LakeCatError::InvalidArgument(_)));
                assert!(message.contains(expected));
                assert!(message.contains("server-endpoint-url-hash=sha256:"));
                assert!(
                    !message.contains(endpoint_url),
                    "server endpoint validation must not expose raw endpoint URLs"
                );
            }
        }

        #[tokio::test]
        async fn server_upsert_rejects_deserialized_invalid_endpoint_urls() {
            let record = ServerRecord {
                server_id: "prod".to_string(),
                display_name: Some("Production".to_string()),
                endpoint_url: Some("s3://lakecat-demo/catalog".to_string()),
                properties: BTreeMap::new(),
                created: AuditStamp::now(Principal::anonymous()),
                updated_at: Utc::now(),
            };

            let memory_err = MemoryCatalogStore::new()
                .upsert_server(record.clone())
                .await
                .unwrap_err();
            let message = memory_err.to_string();
            assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
            assert!(message.contains("http or https scheme"));
            assert!(message.contains("server-endpoint-url-hash=sha256:"));
            assert!(!message.contains("s3://lakecat-demo/catalog"));

            let turso = TursoCatalogStore::in_memory().await.unwrap();
            let turso_err = turso.upsert_server(record).await.unwrap_err();
            let message = turso_err.to_string();
            assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
            assert!(message.contains("http or https scheme"));
            assert!(message.contains("server-endpoint-url-hash=sha256:"));
            assert!(!message.contains("s3://lakecat-demo/catalog"));
            assert_eq!(turso.list_servers().await.unwrap(), vec![]);
        }

        #[tokio::test]
        async fn server_upsert_rejects_deserialized_decorated_endpoint_urls() {
            let record = ServerRecord {
                server_id: "prod".to_string(),
                display_name: Some("Production".to_string()),
                endpoint_url: Some("https://lakecat.example.com?token=abc".to_string()),
                properties: BTreeMap::new(),
                created: AuditStamp::now(Principal::anonymous()),
                updated_at: Utc::now(),
            };

            let memory_err = MemoryCatalogStore::new()
                .upsert_server(record.clone())
                .await
                .unwrap_err();
            let message = memory_err.to_string();
            assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
            assert!(message.contains("query strings, fragments, or userinfo"));
            assert!(message.contains("server-endpoint-url-hash=sha256:"));
            assert!(!message.contains("https://lakecat.example.com?token=abc"));
            assert!(!message.contains("token=abc"));

            let turso = TursoCatalogStore::in_memory().await.unwrap();
            let turso_err = turso.upsert_server(record).await.unwrap_err();
            let message = turso_err.to_string();
            assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
            assert!(message.contains("query strings, fragments, or userinfo"));
            assert!(message.contains("server-endpoint-url-hash=sha256:"));
            assert!(!message.contains("https://lakecat.example.com?token=abc"));
            assert!(!message.contains("token=abc"));
            assert_eq!(turso.list_servers().await.unwrap(), vec![]);
        }

        #[test]
        fn warehouses_reject_dot_segment_storage_roots() {
            let warehouse = WarehouseName::new("local").unwrap();
            for storage_root in [
                "file:///tmp/lakecat/../private",
                "file:///tmp/lakecat/%2e%2e/private",
                "s3://lakecat-demo/root/%2E/private",
            ] {
                let err = WarehouseRecord::new(
                    warehouse.clone(),
                    "default",
                    Some(storage_root.to_string()),
                    BTreeMap::new(),
                    Principal::anonymous(),
                )
                .unwrap_err();

                let message = err.to_string();
                assert!(matches!(err, LakeCatError::InvalidArgument(_)));
                assert!(message.contains("dot path segments"));
                assert!(message.contains("warehouse-storage-root-hash=sha256:"));
                assert!(
                    !message.contains(storage_root),
                    "warehouse dot-segment storage-root validation must not expose raw storage roots"
                );
            }
        }

        #[tokio::test]
        async fn warehouse_upsert_rejects_deserialized_decorated_storage_roots() {
            let warehouse = WarehouseName::new("decorated_root").unwrap();
            let record = WarehouseRecord {
                warehouse: warehouse.clone(),
                project_id: "default".to_string(),
                storage_root: Some("file:///tmp/lakecat?token=abc".to_string()),
                properties: BTreeMap::new(),
                created: AuditStamp::now(Principal::anonymous()),
                updated_at: Utc::now(),
            };

            let memory_err = MemoryCatalogStore::new()
                .upsert_warehouse(record.clone())
                .await
                .unwrap_err();
            let message = memory_err.to_string();
            assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
            assert!(message.contains("query strings, fragments, or userinfo"));
            assert!(message.contains("warehouse-storage-root-hash=sha256:"));
            assert!(!message.contains("file:///tmp/lakecat?token=abc"));
            assert!(!message.contains("token=abc"));

            let turso = TursoCatalogStore::in_memory().await.unwrap();
            let turso_err = turso.upsert_warehouse(record).await.unwrap_err();
            let message = turso_err.to_string();
            assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
            assert!(message.contains("query strings, fragments, or userinfo"));
            assert!(message.contains("warehouse-storage-root-hash=sha256:"));
            assert!(!message.contains("file:///tmp/lakecat?token=abc"));
            assert!(!message.contains("token=abc"));
            assert!(matches!(
                turso.load_warehouse(&warehouse).await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "warehouse" && name == "decorated_root"
            ));
        }

        #[tokio::test]
        async fn warehouse_upsert_rejects_deserialized_dot_segment_storage_roots() {
            let warehouse = WarehouseName::new("dot_root").unwrap();
            let record = WarehouseRecord {
                warehouse: warehouse.clone(),
                project_id: "default".to_string(),
                storage_root: Some("file:///tmp/lakecat/../private".to_string()),
                properties: BTreeMap::new(),
                created: AuditStamp::now(Principal::anonymous()),
                updated_at: Utc::now(),
            };

            let memory_err = MemoryCatalogStore::new()
                .upsert_warehouse(record.clone())
                .await
                .unwrap_err();
            assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
            assert!(memory_err.to_string().contains("dot path segments"));
            assert!(
                memory_err
                    .to_string()
                    .contains("warehouse-storage-root-hash=sha256:")
            );

            let turso = TursoCatalogStore::in_memory().await.unwrap();
            let turso_err = turso.upsert_warehouse(record).await.unwrap_err();
            assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
            assert!(turso_err.to_string().contains("dot path segments"));
            assert!(
                turso_err
                    .to_string()
                    .contains("warehouse-storage-root-hash=sha256:")
            );
            assert!(matches!(
                turso.load_warehouse(&warehouse).await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "warehouse" && name == "dot_root"
            ));
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

        #[test]
        fn storage_profiles_reject_decorated_secret_ref_uris() {
            let warehouse = WarehouseName::new("local").unwrap();
            for (secret_ref, expected) in [
                (
                    "typesec://lakecat/local/s3-events?version=1",
                    "query strings",
                ),
                ("vault://token@secret/data/lakecat/s3-events", "userinfo"),
                ("aws-sm://lakecat/s3-events#current", "fragments"),
            ] {
                let err = StorageProfile::new(
                    "decorated-secret-ref",
                    warehouse.clone(),
                    "s3://lakecat-demo/events",
                    StorageProvider::S3,
                    CredentialIssuanceMode::ShortLivedSecretRef,
                    Some(secret_ref.to_string()),
                    BTreeMap::new(),
                )
                .unwrap_err();

                assert!(matches!(err, LakeCatError::InvalidArgument(_)));
                let message = err.to_string();
                assert!(
                    message.contains(expected),
                    "expected {secret_ref} to reject {expected}, got {err}"
                );
                assert!(message.contains("secret-ref-hash=sha256:"));
                assert!(
                    !message.contains(secret_ref),
                    "decorated secret-ref validation must not expose raw secret refs"
                );
            }
        }

        #[test]
        fn storage_profiles_redact_invalid_secret_ref_uris() {
            let warehouse = WarehouseName::new("local").unwrap();
            for secret_ref in [
                "not a uri with secret=abc",
                "file:///tmp/raw-secret",
                "postgres://user:secret@example.test/credentials",
            ] {
                let err = StorageProfile::new(
                    "invalid-secret-ref",
                    warehouse.clone(),
                    "s3://lakecat-demo/events",
                    StorageProvider::S3,
                    CredentialIssuanceMode::ShortLivedSecretRef,
                    Some(secret_ref.to_string()),
                    BTreeMap::new(),
                )
                .unwrap_err();

                let message = err.to_string();
                assert!(matches!(err, LakeCatError::InvalidArgument(_)));
                assert!(message.contains("secret-ref-hash=sha256:"));
                assert!(
                    !message.contains(secret_ref),
                    "storage profile validation errors must not expose raw secret refs"
                );
            }
        }

        #[test]
        fn storage_profiles_reject_dot_segment_secret_refs() {
            let warehouse = WarehouseName::new("local").unwrap();
            for secret_ref in [
                "vault://secret/data/lakecat/../s3-events",
                "aws-sm://lakecat/%2e%2e/s3-events",
                "gcp-sm://lakecat/%2E/s3-events",
            ] {
                let err = StorageProfile::new(
                    "dot-secret-ref",
                    warehouse.clone(),
                    "s3://lakecat-demo/events",
                    StorageProvider::S3,
                    CredentialIssuanceMode::ShortLivedSecretRef,
                    Some(secret_ref.to_string()),
                    BTreeMap::new(),
                )
                .unwrap_err();

                let message = err.to_string();
                assert!(matches!(err, LakeCatError::InvalidArgument(_)));
                assert!(message.contains("dot path segments"));
                assert!(message.contains("secret-ref-hash=sha256:"));
                assert!(
                    !message.contains(secret_ref),
                    "dot-segment secret-ref validation must not expose raw secret refs"
                );
            }
        }

        #[test]
        fn storage_profiles_redact_embedded_secret_ref_material() {
            let warehouse = WarehouseName::new("local").unwrap();
            let secret_ref = "vault://secret/data/lakecat/s3-events/password=abc";
            let err = StorageProfile::new(
                "embedded-secret-ref",
                warehouse,
                "s3://lakecat-demo/events",
                StorageProvider::S3,
                CredentialIssuanceMode::ShortLivedSecretRef,
                Some(secret_ref.to_string()),
                BTreeMap::new(),
            )
            .unwrap_err();

            let message = err.to_string();
            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            assert!(message.contains("must not embed raw secret material"));
            assert!(message.contains("secret-ref-hash=sha256:"));
            assert!(
                !message.contains(secret_ref),
                "embedded secret-ref validation must not expose raw secret refs"
            );
            assert!(!message.contains("password=abc"));
        }

        #[test]
        fn secret_ref_dot_segment_detection_allows_ordinary_dotted_names() {
            assert!(crate::secret_ref_has_dot_path_segment(
                "vault://secret/data/lakecat/../s3-events"
            ));
            assert!(crate::secret_ref_has_dot_path_segment(
                "vault://secret/data/lakecat/%2e%2e/s3-events"
            ));
            assert!(!crate::secret_ref_has_dot_path_segment(
                "vault://secret/data/lakecat/service.v1/s3-events"
            ));
        }

        #[test]
        fn storage_profiles_reject_provider_location_mismatch() {
            let warehouse = WarehouseName::new("local").unwrap();
            let err = StorageProfile::new(
                "wrong-provider",
                warehouse,
                "s3://lakecat-demo/events",
                StorageProvider::File,
                CredentialIssuanceMode::LocalFileNoSecret,
                None,
                BTreeMap::new(),
            )
            .unwrap_err();

            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("does not match location prefix provider"));
            assert!(message.contains("storage-profile-prefix-hash=sha256:"));
            assert!(!message.contains("s3://lakecat-demo/events"));
            assert!(!message.contains("lakecat-demo"));
        }

        #[test]
        fn storage_profiles_redact_unsupported_provider_location_prefixes() {
            let warehouse = WarehouseName::new("local").unwrap();
            let err = StorageProfile::new(
                "unsupported-prefix",
                warehouse,
                "https://lakecat-demo.example/events",
                StorageProvider::S3,
                CredentialIssuanceMode::ShortLivedSecretRef,
                Some("vault://kv/lakecat/events".to_string()),
                BTreeMap::new(),
            )
            .unwrap_err();

            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("is not supported by provider 's3'"));
            assert!(message.contains("storage-profile-prefix-hash=sha256:"));
            assert!(!message.contains("https://lakecat-demo.example/events"));
            assert!(!message.contains("lakecat-demo"));
        }

        #[test]
        fn storage_profiles_reject_provider_issuance_mismatch() {
            let warehouse = WarehouseName::new("local").unwrap();
            let remote_no_secret = StorageProfile::new(
                "remote-no-secret",
                warehouse.clone(),
                "s3://lakecat-demo/events",
                StorageProvider::S3,
                CredentialIssuanceMode::LocalFileNoSecret,
                None,
                BTreeMap::new(),
            )
            .unwrap_err();
            assert!(matches!(remote_no_secret, LakeCatError::InvalidArgument(_)));
            assert!(
                remote_no_secret
                    .to_string()
                    .contains("local-file-no-secret issuance mode requires file provider")
            );
            let remote_message = remote_no_secret.to_string();
            assert!(remote_message.contains("storage-profile-prefix-hash=sha256:"));
            assert!(!remote_message.contains("s3://lakecat-demo/events"));
            assert!(!remote_message.contains("lakecat-demo"));

            let local_secret_ref = StorageProfile::new(
                "local-secret-ref",
                warehouse,
                "file:///tmp/events",
                StorageProvider::File,
                CredentialIssuanceMode::ShortLivedSecretRef,
                Some("typesec://lakecat/local/events".to_string()),
                BTreeMap::new(),
            )
            .unwrap_err();
            assert!(matches!(local_secret_ref, LakeCatError::InvalidArgument(_)));
            assert!(
                local_secret_ref
                    .to_string()
                    .contains("short-lived-secret-ref issuance mode requires s3, gcs, or azure")
            );
            let local_message = local_secret_ref.to_string();
            assert!(local_message.contains("storage-profile-prefix-hash=sha256:"));
            assert!(!local_message.contains("file:///tmp/events"));
            assert!(!local_message.contains("typesec://lakecat/local/events"));
        }

        #[test]
        fn storage_profiles_reject_public_config_secret_values() {
            let warehouse = WarehouseName::new("local").unwrap();
            let err = StorageProfile::new(
                "secret-public-config",
                warehouse,
                "s3://lakecat-demo/events",
                StorageProvider::S3,
                CredentialIssuanceMode::ShortLivedSecretRef,
                Some("typesec://lakecat/local/s3-events".to_string()),
                BTreeMap::from([(
                    "lakecat.endpoint".to_string(),
                    "https://storage.example.invalid?token=raw-secret".to_string(),
                )]),
            )
            .unwrap_err();

            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("public config value may expose secret material"));
            assert!(message.contains("public-config-key-hash=sha256:"));
            assert!(!message.contains("lakecat.endpoint"));
            assert!(!message.contains("raw-secret"));
        }

        #[test]
        fn storage_profile_validate_rejects_public_config_secret_values() {
            let profile = StorageProfile {
                profile_id: "secret-public-config".to_string(),
                warehouse: WarehouseName::new("local").unwrap(),
                location_prefix: "s3://lakecat-demo/events".to_string(),
                provider: StorageProvider::S3,
                issuance_mode: CredentialIssuanceMode::ShortLivedSecretRef,
                secret_ref: Some("typesec://lakecat/local/s3-events".to_string()),
                public_config: BTreeMap::from([(
                    "lakecat.endpoint".to_string(),
                    "https://storage.example.invalid?token=raw-secret".to_string(),
                )]),
            };

            let err = profile.validate().unwrap_err();
            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("public config value may expose secret material"));
            assert!(message.contains("public-config-key-hash=sha256:"));
            assert!(!message.contains("lakecat.endpoint"));
            assert!(!message.contains("raw-secret"));
        }

        #[test]
        fn storage_profiles_redact_secret_like_public_config_keys() {
            let err = StorageProfile::new(
                "secret-key-public-config",
                WarehouseName::new("local").unwrap(),
                "file:///tmp/events",
                StorageProvider::File,
                CredentialIssuanceMode::LocalFileNoSecret,
                None,
                BTreeMap::from([(
                    "customer-secret-token".to_string(),
                    "metadata-only".to_string(),
                )]),
            )
            .unwrap_err();

            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("public config key may expose secret material"));
            assert!(message.contains("public-config-key-hash=sha256:"));
            assert!(!message.contains("customer-secret-token"));
        }

        #[test]
        fn storage_profiles_reject_reserved_public_config_keys() {
            let err = StorageProfile::new(
                "reserved-public-config",
                WarehouseName::new("local").unwrap(),
                "file:///tmp/events",
                StorageProvider::File,
                CredentialIssuanceMode::LocalFileNoSecret,
                None,
                BTreeMap::from([(
                    "lakecat.storage-profile-id".to_string(),
                    "shadow-profile".to_string(),
                )]),
            )
            .unwrap_err();

            assert!(matches!(err, LakeCatError::InvalidArgument(_)));
            let message = err.to_string();
            assert!(message.contains("reserved for LakeCat credential evidence"));
            assert!(message.contains("public-config-key-hash=sha256:"));
            assert!(!message.contains("lakecat.storage-profile-id"));
        }

        #[tokio::test]
        async fn storage_profile_upsert_rejects_deserialized_public_config_secrets() {
            let warehouse = WarehouseName::new("local").unwrap();
            let profile = StorageProfile {
                profile_id: "secret-public-config".to_string(),
                warehouse: warehouse.clone(),
                location_prefix: "s3://lakecat-demo/events".to_string(),
                provider: StorageProvider::S3,
                issuance_mode: CredentialIssuanceMode::ShortLivedSecretRef,
                secret_ref: Some("typesec://lakecat/local/s3-events".to_string()),
                public_config: BTreeMap::from([(
                    "lakecat.endpoint".to_string(),
                    "https://storage.example.invalid?token=raw-secret".to_string(),
                )]),
            };

            let memory_err = MemoryCatalogStore::new()
                .upsert_storage_profile(profile.clone())
                .await
                .unwrap_err();
            assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
            let memory_message = memory_err.to_string();
            assert!(memory_message.contains("public config value may expose secret material"));
            assert!(memory_message.contains("public-config-key-hash=sha256:"));
            assert!(!memory_message.contains("lakecat.endpoint"));
            assert!(!memory_message.contains("raw-secret"));

            let turso = TursoCatalogStore::in_memory().await.unwrap();
            let turso_err = turso.upsert_storage_profile(profile).await.unwrap_err();
            assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
            let turso_message = turso_err.to_string();
            assert!(turso_message.contains("public config value may expose secret material"));
            assert!(turso_message.contains("public-config-key-hash=sha256:"));
            assert!(!turso_message.contains("lakecat.endpoint"));
            assert!(!turso_message.contains("raw-secret"));
            assert_eq!(
                turso.list_storage_profiles(&warehouse).await.unwrap(),
                vec![]
            );
        }

        #[tokio::test]
        async fn storage_profile_upsert_rejects_reserved_public_config_keys() {
            let warehouse = WarehouseName::new("local").unwrap();
            let profile = StorageProfile {
                profile_id: "reserved-public-config".to_string(),
                warehouse: warehouse.clone(),
                location_prefix: "file:///tmp/events".to_string(),
                provider: StorageProvider::File,
                issuance_mode: CredentialIssuanceMode::LocalFileNoSecret,
                secret_ref: None,
                public_config: BTreeMap::from([(
                    "lakecat.storage-profile-id".to_string(),
                    "shadow-profile".to_string(),
                )]),
            };

            let memory_err = MemoryCatalogStore::new()
                .upsert_storage_profile(profile.clone())
                .await
                .unwrap_err();
            assert!(matches!(memory_err, LakeCatError::InvalidArgument(_)));
            let memory_message = memory_err.to_string();
            assert!(memory_message.contains("reserved for LakeCat credential evidence"));
            assert!(memory_message.contains("public-config-key-hash=sha256:"));
            assert!(!memory_message.contains("lakecat.storage-profile-id"));

            let turso = TursoCatalogStore::in_memory().await.unwrap();
            let turso_err = turso.upsert_storage_profile(profile).await.unwrap_err();
            assert!(matches!(turso_err, LakeCatError::InvalidArgument(_)));
            let turso_message = turso_err.to_string();
            assert!(turso_message.contains("reserved for LakeCat credential evidence"));
            assert!(turso_message.contains("public-config-key-hash=sha256:"));
            assert!(!turso_message.contains("lakecat.storage-profile-id"));
            assert_eq!(
                turso.list_storage_profiles(&warehouse).await.unwrap(),
                vec![]
            );
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
        async fn turso_store_rejects_deserialized_invalid_policy_bindings() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let binding = PolicyBinding {
                policy_id: "table-policy".to_string(),
                warehouse: warehouse.clone(),
                namespace: None,
                table: Some(TableName::new("events").unwrap()),
                enforced: true,
                odrl: serde_json::json!({"uid": "policy:table-policy"}),
                updated_at: Utc::now(),
            };

            let err = store.upsert_policy_binding(binding).await.unwrap_err();

            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table-scoped policy binding requires namespace")
            ));
            assert_eq!(
                store.list_policy_bindings(&warehouse).await.unwrap(),
                vec![]
            );
        }

        #[tokio::test]
        async fn turso_store_rejects_corrupt_policy_bindings_on_read() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let table = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            let mut binding = PolicyBinding::new(
                "table-policy",
                warehouse.clone(),
                Some(namespace),
                Some(TableName::new("events").unwrap()),
                true,
                serde_json::json!({"uid": "policy:table-policy"}),
            )
            .unwrap();
            store.upsert_policy_binding(binding.clone()).await.unwrap();
            binding.namespace = None;

            let conn = store.connect().unwrap();
            conn.execute(
                "update policy_bindings set binding_json = ?2 where policy_key = ?1",
                (
                    policy_binding_key(&warehouse, "table-policy"),
                    encode_json(&binding).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store.list_policy_bindings(&warehouse).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table-scoped policy binding requires namespace")
            ));

            let err = store.policy_bindings_for_table(&table).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("table-scoped policy binding requires namespace")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_policy_binding_json_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let table = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            let mut binding = PolicyBinding::new(
                "table-policy",
                warehouse.clone(),
                Some(namespace),
                Some(TableName::new("events").unwrap()),
                true,
                serde_json::json!({"uid": "policy:table-policy"}),
            )
            .unwrap();
            store.upsert_policy_binding(binding.clone()).await.unwrap();
            binding.policy_id = "other-policy".to_string();

            let conn = store.connect().unwrap();
            conn.execute(
                "update policy_bindings set binding_json = ?2 where policy_key = ?1",
                (
                    policy_binding_key(&warehouse, "table-policy"),
                    encode_json(&binding).unwrap(),
                ),
            )
            .await
            .unwrap();

            let err = store.list_policy_bindings(&warehouse).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("policy binding row scope does not match")
            ));

            let err = store.policy_bindings_for_table(&table).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("policy binding row scope does not match")
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_policy_binding_row_column_scope_drift() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let table = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
                TableName::new("events").unwrap(),
            );
            let binding = PolicyBinding::new(
                "table-policy",
                warehouse.clone(),
                Some(namespace),
                Some(TableName::new("events").unwrap()),
                true,
                serde_json::json!({"uid": "policy:table-policy"}),
            )
            .unwrap();
            store.upsert_policy_binding(binding).await.unwrap();

            let conn = store.connect().unwrap();
            conn.execute(
                "update policy_bindings
                 set namespace_path = ?2, table_name = ?3, enforced = ?4
                 where policy_key = ?1",
                (
                    policy_binding_key(&warehouse, "table-policy"),
                    "other",
                    "other_events",
                    0_i64,
                ),
            )
            .await
            .unwrap();

            let err = store.list_policy_bindings(&warehouse).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("policy binding row scope does not match")
            ));

            let err = store.policy_bindings_for_table(&table).await.unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("policy binding row scope does not match")
            ));
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

        #[tokio::test]
        async fn turso_store_rejects_corrupt_soft_delete_records_on_restore() {
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

            let corrupt = SoftDeleteRecord {
                table: ident.clone(),
                metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
                version: 1,
                format_version: Some(3),
                principal: Principal::anonymous(),
                authorization_receipt: Some(serde_json::json!({
                    "engine": "typesec",
                    "allowed": true,
                    "action": "table-drop"
                })),
                deleted_at: Utc::now(),
            };
            let conn = store.connect().unwrap();
            conn.execute(
                "update soft_deletes set record_json = ?2 where table_key = ?1",
                (table_key(&ident), encode_json(&corrupt).unwrap()),
            )
            .await
            .unwrap();

            let err = store
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
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("soft-delete version does not match table record")
            ));
            assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 1);
            assert!(matches!(
                store.load_table(&ident).await,
                Err(LakeCatError::NotFound { .. })
            ));
        }

        #[tokio::test]
        async fn turso_store_rejects_soft_delete_row_scope_drift_on_restore() {
            let store = TursoCatalogStore::in_memory().await.unwrap();
            let warehouse = WarehouseName::new("local").unwrap();
            let namespace = "default".parse::<Namespace>().unwrap();
            let ident = TableIdent::new(
                warehouse.clone(),
                namespace.clone(),
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

            let conn = store.connect().unwrap();
            conn.execute(
                "update soft_deletes set namespace_path = ?2 where table_key = ?1",
                (table_key(&ident), "other_namespace"),
            )
            .await
            .unwrap();

            let err = store
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
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::Internal(message)
                    if message.contains("soft-delete row scope does not match")
            ));
            assert_eq!(store.count_rows("soft_deletes").await.unwrap(), 1);
            assert!(matches!(
                store.load_table(&ident).await,
                Err(LakeCatError::NotFound { .. })
            ));

            let row_key_store = TursoCatalogStore::in_memory().await.unwrap();
            let other_ident = TableIdent::new(
                warehouse.clone(),
                namespace,
                TableName::new("other_events").unwrap(),
            );
            for table_ident in [&ident, &other_ident] {
                row_key_store
                    .create_table(TableRecord::new(
                        table_ident.clone(),
                        format!("file:///tmp/{}", table_ident.name),
                        Some(format!(
                            "file:///tmp/{}/metadata/00000.json",
                            table_ident.name
                        )),
                        serde_json::json!({"format-version": 3}),
                        Principal::anonymous(),
                    ))
                    .await
                    .unwrap();
            }
            row_key_store
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

            let conn = row_key_store.connect().unwrap();
            conn.execute(
                "update soft_deletes set table_key = ?2 where table_key = ?1",
                (table_key(&ident), table_key(&other_ident)),
            )
            .await
            .unwrap();

            assert!(matches!(
                row_key_store
                    .restore_table(
                        &ident,
                        Principal::anonymous(),
                        Some(serde_json::json!({
                            "engine": "typesec",
                            "allowed": true,
                            "action": "table-restore"
                        })),
                    )
                    .await,
                Err(LakeCatError::NotFound { object, name })
                    if object == "soft-deleted table" && name == ident.stable_id()
            ));
            let err = row_key_store
                .restore_table(
                    &other_ident,
                    Principal::anonymous(),
                    Some(serde_json::json!({
                        "engine": "typesec",
                        "allowed": true,
                        "action": "table-restore"
                    })),
                )
                .await
                .unwrap_err();
            assert!(matches!(
                err,
                LakeCatError::InvalidArgument(message)
                    if message.contains("soft-delete record table does not match requested table")
            ));
            assert_eq!(row_key_store.count_rows("soft_deletes").await.unwrap(), 1);
        }
    }
}
