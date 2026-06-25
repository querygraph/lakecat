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
        validate_commit_record_metadata_location(
            self.previous_metadata_location.as_deref(),
            "previous",
        )?;
        validate_commit_record_metadata_location(self.new_metadata_location.as_deref(), "new")?;
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

fn validate_commit_record_metadata_location(
    location: Option<&str>,
    label: &str,
) -> LakeCatResult<()> {
    let Some(location) = location else {
        return Ok(());
    };
    if location_has_query_fragment_or_userinfo(location) {
        return Err(LakeCatError::Internal(format!(
            "table commit record {label} metadata location must not contain decorated location material; metadata-location-hash={}",
            content_hash_bytes(location.as_bytes())
        )));
    }
    if embeds_raw_secret_material(location) {
        return Err(LakeCatError::Internal(format!(
            "table commit record {label} metadata location must not contain credential material; metadata-location-hash={}",
            content_hash_bytes(location.as_bytes())
        )));
    }
    Ok(())
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

fn validate_memory_view_receipt_scope(
    record: &MemoryViewVersionReceipt,
    warehouse: &WarehouseName,
    namespace: &Namespace,
    view: Option<&TableName>,
) -> LakeCatResult<()> {
    let expected_key = view_key_parts(
        &record.receipt.warehouse,
        &record.receipt.namespace,
        &record.receipt.name,
    );
    if record.view_key != expected_key {
        return Err(LakeCatError::Internal(
            "view receipt row scope does not match receipt identity".to_string(),
        ));
    }
    if record.receipt.warehouse != *warehouse || record.receipt.namespace != *namespace {
        return Err(LakeCatError::Internal(
            "view receipt row scope does not match receipt identity".to_string(),
        ));
    }
    if let Some(view) = view
        && record.receipt.name != *view
    {
        return Err(LakeCatError::Internal(
            "view receipt row scope does not match receipt identity".to_string(),
        ));
    }
    record.receipt.validate()?;
    Ok(())
}

fn memory_view_receipt_key_matches_namespace(
    view_key: &str,
    warehouse: &WarehouseName,
    namespace: &Namespace,
) -> bool {
    let mut parts = view_key.split('\u{1f}');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(row_warehouse), Some(row_namespace), Some(_), None)
            if row_warehouse == warehouse.as_str() && row_namespace == namespace.path()
    )
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

fn validate_view_record_map_scope(record: &ViewRecord, record_key: &str) -> LakeCatResult<()> {
    record.validate()?;
    if view_key(record) != record_key {
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
        validate_secret_ref_issuance_mode(secret_ref.as_deref(), issuance_mode)?;
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
        validate_secret_ref_issuance_mode(self.secret_ref.as_deref(), self.issuance_mode)?;
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
    commits: Vec<MemoryCommitRecord>,
    audit_events: Vec<CatalogAuditEvent>,
    outbox_events: Vec<OutboxEvent>,
    idempotency: BTreeMap<String, IdempotencyReplay>,
    storage_profiles: BTreeMap<String, StorageProfile>,
    views: BTreeMap<String, ViewRecord>,
    view_version_receipts: Vec<MemoryViewVersionReceipt>,
    policy_bindings: BTreeMap<String, PolicyBinding>,
    soft_deletes: BTreeMap<String, SoftDeleteRecord>,
}

#[derive(Debug, Clone)]
struct IdempotencyReplay {
    table_key: String,
    request_hash: String,
    response: TableRecord,
}

#[derive(Debug, Clone)]
struct MemoryCommitRecord {
    table_key: String,
    record: TableCommitRecord,
}

#[derive(Debug, Clone)]
struct MemoryViewVersionReceipt {
    view_key: String,
    receipt: ViewVersionReceipt,
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
            .iter()
            .map(|(table_key, table)| {
                validate_table_record_map_scope(table, table_key)?;
                Ok(table)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .any(|table| table.ident.warehouse == *warehouse && table.ident.namespace == *namespace)
        {
            return Err(namespace_not_empty(namespace, "tables"));
        }
        if state
            .views
            .iter()
            .map(|(view_key, view)| {
                validate_view_record_map_scope(view, view_key)?;
                Ok(view)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .any(|view| view.warehouse == *warehouse && view.namespace == *namespace)
        {
            return Err(namespace_not_empty(namespace, "views"));
        }
        if state
            .policy_bindings
            .iter()
            .map(|(binding_key, binding)| {
                validate_policy_binding_map_scope(binding, binding_key)?;
                Ok(binding)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .any(|binding| {
                binding.warehouse == *warehouse && binding.namespace.as_ref() == Some(namespace)
            })
        {
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
            .iter()
            .map(|(table_key, table)| {
                validate_table_record_map_scope(table, table_key)?;
                Ok(table)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
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
            .and_then(|table| {
                validate_table_record_map_scope(&table, &table_key(ident))?;
                validate_table_record_identity(&table, ident)?;
                Ok(table)
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
                validate_idempotency_record_table_key(&replay.table_key, ident)?;
                validate_idempotency_record_request_hash(&replay.request_hash)?;
                if replay.request_hash == idempotency_request_hash {
                    validate_table_record_identity(&replay.response, ident)?;
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
            validate_table_record_map_scope(table, &key)?;
            validate_table_record_identity(table, ident)?;
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
        let audit_payload_hash = content_hash_json(&audit_payload)?;
        let outbox_payload = serde_json::json!({
            "audit-event-id": audit_payload_hash,
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
            request_hash: Some(audit_payload_hash),
            payload: audit_payload,
            created_at: committed_at,
        });
        state.outbox_events.push(outbox_event);
        state.commits.push(MemoryCommitRecord {
            table_key: table_key(ident),
            record,
        });

        if let Some(idempotency_key) = commit.idempotency_key {
            state.idempotency.insert(
                format!("{}:{idempotency_key}", ident.stable_id()),
                IdempotencyReplay {
                    table_key: table_key(ident),
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
        validate_idempotency_record_table_key(&replay.table_key, ident)?;
        validate_idempotency_record_request_hash(&replay.request_hash)?;
        if replay.request_hash != idempotency_request_hash {
            return Err(LakeCatError::Conflict(format!(
                "idempotency key reused with different commit request for {}",
                ident.stable_id()
            )));
        }
        validate_table_record_identity(&replay.response, ident)?;
        Ok(Some(replay.response.clone()))
    }

    async fn table_commit_records(
        &self,
        ident: &TableIdent,
        start_version: u64,
        end_version: Option<u64>,
    ) -> LakeCatResult<Vec<TableCommitRecord>> {
        let state = self.state.read().await;
        let key = table_key(ident);
        state
            .commits
            .iter()
            .filter(|commit| commit.table_key == key)
            .filter(|commit| commit.record.sequence_number >= start_version)
            .filter(|commit| end_version.is_none_or(|end| commit.record.sequence_number <= end))
            .map(|commit| {
                validate_table_commit_record_memory_scope(commit, ident)?;
                Ok(commit.record.clone())
            })
            .collect()
    }

    async fn upsert_server(&self, server: ServerRecord) -> LakeCatResult<ServerRecord> {
        server.validate()?;
        let mut state = self.state.write().await;
        if let Some(existing) = state.servers.get(&server.server_id) {
            validate_server_record_map_scope(existing, &server.server_id)?;
        }
        state
            .servers
            .insert(server.server_id.clone(), server.clone());
        Ok(server)
    }

    async fn list_servers(&self) -> LakeCatResult<Vec<ServerRecord>> {
        let state = self.state.read().await;
        let mut servers = state
            .servers
            .iter()
            .map(|(server_id, server)| {
                validate_server_record_map_scope(server, server_id)?;
                Ok(server.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        servers.sort_by(|left, right| left.server_id.cmp(&right.server_id));
        Ok(servers)
    }

    async fn upsert_project(&self, project: ProjectRecord) -> LakeCatResult<ProjectRecord> {
        project.validate()?;
        let mut state = self.state.write().await;
        if let Some(server_id) = project.server_id.as_deref() {
            let Some(server) = state.servers.get(server_id) else {
                return Err(LakeCatError::NotFound {
                    object: "server",
                    name: server_id.to_string(),
                });
            };
            validate_server_record_map_scope(server, server_id)?;
        }
        if let Some(existing) = state.projects.get(&project.project_id) {
            validate_project_record_map_scope(existing, &project.project_id)?;
        }
        state
            .projects
            .insert(project.project_id.clone(), project.clone());
        Ok(project)
    }

    async fn list_projects(&self) -> LakeCatResult<Vec<ProjectRecord>> {
        let state = self.state.read().await;
        let mut projects = state
            .projects
            .iter()
            .map(|(project_id, project)| {
                validate_project_record_map_scope(project, project_id)?;
                Ok(project.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        projects.sort_by(|left, right| left.project_id.cmp(&right.project_id));
        Ok(projects)
    }

    async fn upsert_warehouse(&self, warehouse: WarehouseRecord) -> LakeCatResult<WarehouseRecord> {
        warehouse.validate()?;
        let mut state = self.state.write().await;
        let Some(project) = state.projects.get(&warehouse.project_id) else {
            return Err(LakeCatError::NotFound {
                object: "project",
                name: warehouse.project_id.clone(),
            });
        };
        validate_project_record_map_scope(project, &warehouse.project_id)?;
        let warehouse_key = warehouse.warehouse.as_str().to_string();
        if let Some(existing) = state.warehouses.get(&warehouse_key) {
            validate_warehouse_record_map_scope(existing, &warehouse_key)?;
        }
        state.warehouses.insert(warehouse_key, warehouse.clone());
        Ok(warehouse)
    }

    async fn load_warehouse(&self, warehouse: &WarehouseName) -> LakeCatResult<WarehouseRecord> {
        let state = self.state.read().await;
        let warehouse_key = warehouse.as_str().to_string();
        let warehouse = state
            .warehouses
            .get(warehouse_key.as_str())
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "warehouse",
                name: warehouse.as_str().to_string(),
            })?;
        validate_warehouse_record_map_scope(&warehouse, warehouse_key.as_str())?;
        Ok(warehouse)
    }

    async fn list_warehouses(&self) -> LakeCatResult<Vec<WarehouseRecord>> {
        let state = self.state.read().await;
        let mut warehouses = state
            .warehouses
            .iter()
            .map(|(warehouse_key, warehouse)| {
                validate_warehouse_record_map_scope(warehouse, warehouse_key)?;
                Ok(warehouse.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        warehouses.sort_by(|left, right| left.warehouse.as_str().cmp(right.warehouse.as_str()));
        Ok(warehouses)
    }

    async fn list_project_warehouses(
        &self,
        project_id: &str,
    ) -> LakeCatResult<Vec<WarehouseRecord>> {
        validate_project_id(project_id)?;
        let state = self.state.read().await;
        let project = state
            .projects
            .get(project_id)
            .ok_or_else(|| LakeCatError::NotFound {
                object: "project",
                name: project_id.to_string(),
            })?;
        validate_project_record_map_scope(project, project_id)?;
        let mut warehouses = state
            .warehouses
            .iter()
            .map(|(warehouse_key, warehouse)| {
                validate_warehouse_record_map_scope(warehouse, warehouse_key)?;
                Ok(warehouse)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
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
        validate_table_record_map_scope(&table, &key)?;
        validate_table_record_identity(&table, ident)?;
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
        let audit_payload = serde_json::json!({
            "event-type": "table.deleted",
            "table": ident,
            "soft-delete": &record,
            "authorization-receipt": &record.authorization_receipt,
        });
        let audit_payload_hash = content_hash_json(&audit_payload)?;
        let outbox_payload = serde_json::json!({
            "audit-event-id": audit_payload_hash,
            "event-type": "table.deleted",
            "table": ident,
            "soft-delete": audit_payload["soft-delete"].clone(),
            "authorization-receipt": audit_payload["authorization-receipt"].clone(),
        });
        let outbox_event = outbox_event_from_payload(&outbox_payload, record.deleted_at)?;
        let audit_principal = record.principal.clone();
        state.soft_deletes.insert(key, record);
        state.audit_events.push(CatalogAuditEvent {
            event_type: "table.deleted".to_string(),
            table: Some(ident.clone()),
            principal: audit_principal,
            request_hash: Some(audit_payload_hash),
            payload: audit_payload,
            created_at: outbox_event.created_at,
        });
        state.outbox_events.push(outbox_event);
        Ok(table)
    }

    async fn restore_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord> {
        let mut state = self.state.write().await;
        let key = table_key(ident);
        let Some(record) = state.soft_deletes.get(&key) else {
            return Err(LakeCatError::NotFound {
                object: "soft-deleted table",
                name: ident.stable_id(),
            });
        };
        validate_soft_delete_record_map_scope(record, &key)?;
        let table = state
            .tables
            .get(&key)
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })?;
        validate_table_record_map_scope(&table, &key)?;
        validate_table_record_identity(&table, ident)?;
        record.validate_for_table(ident, &table)?;
        let restored_at = Utc::now();
        let audit_payload = serde_json::json!({
            "event-type": "table.restored",
            "table": ident,
            "authorization-receipt": authorization_receipt,
            "metadata-location": table.metadata_location,
            "format-version": table_commit_format_version(&table),
            "version": table.version,
        });
        let audit_payload_hash = content_hash_json(&audit_payload)?;
        let outbox_payload = serde_json::json!({
            "audit-event-id": audit_payload_hash,
            "event-type": "table.restored",
            "table": ident,
            "payload": audit_payload.clone(),
            "authorization-receipt": audit_payload["authorization-receipt"].clone(),
        });
        let outbox_event = outbox_event_from_payload(&outbox_payload, restored_at)?;
        state.soft_deletes.remove(&key);
        state.audit_events.push(CatalogAuditEvent {
            event_type: "table.restored".to_string(),
            table: Some(ident.clone()),
            principal,
            request_hash: Some(audit_payload_hash),
            payload: audit_payload,
            created_at: restored_at,
        });
        state.outbox_events.push(outbox_event);
        Ok(table)
    }

    async fn upsert_storage_profile(
        &self,
        profile: StorageProfile,
    ) -> LakeCatResult<StorageProfile> {
        profile.validate()?;
        let mut state = self.state.write().await;
        let key = storage_profile_key(&profile.warehouse, &profile.profile_id);
        if let Some(existing) = state.storage_profiles.get(&key) {
            validate_storage_profile_map_scope(existing, &key)?;
        }
        state.storage_profiles.insert(key, profile.clone());
        Ok(profile)
    }

    async fn list_storage_profiles(
        &self,
        warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<StorageProfile>> {
        let state = self.state.read().await;
        let mut profiles = state
            .storage_profiles
            .iter()
            .map(|(profile_key, profile)| {
                validate_storage_profile_map_scope(profile, profile_key)?;
                Ok(profile)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
            .filter(|profile| profile.warehouse == *warehouse)
            .cloned()
            .collect::<Vec<_>>();
        profiles.sort_by(|left, right| left.profile_id.cmp(&right.profile_id));
        Ok(profiles)
    }

    async fn upsert_view(&self, view: ViewRecord) -> LakeCatResult<ViewRecord> {
        view.validate()?;
        let mut state = self.state.write().await;
        let view_key = view_key(&view);
        let principal = view.created.principal.clone();
        let previous = state.views.get(&view_key);
        if let Some(previous) = previous {
            validate_view_record_map_scope(previous, &view_key)?;
        }
        let latest_receipt = latest_view_receipt_evidence(
            state
                .view_version_receipts
                .iter()
                .filter(|receipt| receipt.view_key == view_key)
                .map(|receipt| {
                    validate_memory_view_receipt_scope(
                        receipt,
                        &view.warehouse,
                        &view.namespace,
                        Some(&view.name),
                    )?;
                    Ok(&receipt.receipt)
                })
                .collect::<LakeCatResult<Vec<_>>>()?
                .into_iter(),
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
        state.views.insert(view_key.clone(), view.clone());
        state
            .view_version_receipts
            .push(MemoryViewVersionReceipt { view_key, receipt });
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
        if let Some(previous) = previous {
            validate_view_record_map_scope(previous, &view_key)?;
        }
        if let Some(expected) = expected_view_version {
            require_expected_view_version(previous, expected)?;
        }
        let latest_receipt = latest_view_receipt_evidence(
            state
                .view_version_receipts
                .iter()
                .filter(|receipt| receipt.view_key == view_key)
                .map(|receipt| {
                    validate_memory_view_receipt_scope(
                        receipt,
                        &view.warehouse,
                        &view.namespace,
                        Some(&view.name),
                    )?;
                    Ok(&receipt.receipt)
                })
                .collect::<LakeCatResult<Vec<_>>>()?
                .into_iter(),
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
        state.views.insert(view_key.clone(), view.clone());
        state
            .view_version_receipts
            .push(MemoryViewVersionReceipt { view_key, receipt });
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
            .filter(|receipt| receipt.view_key == view_key_parts(warehouse, namespace, view))
            .map(|receipt| {
                validate_memory_view_receipt_scope(receipt, warehouse, namespace, Some(view))?;
                Ok(receipt.receipt.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
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
            .filter(|receipt| {
                memory_view_receipt_key_matches_namespace(&receipt.view_key, warehouse, namespace)
            })
            .map(|receipt| {
                validate_memory_view_receipt_scope(receipt, warehouse, namespace, None)?;
                Ok(receipt.receipt.clone())
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
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
                validate_view_record_map_scope(
                    &record,
                    &view_key_parts(warehouse, namespace, view),
                )?;
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
        validate_view_record_map_scope(current, &view_key)?;
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
                .filter(|receipt| receipt.view_key == view_key)
                .map(|receipt| {
                    validate_memory_view_receipt_scope(receipt, warehouse, namespace, Some(view))?;
                    Ok(&receipt.receipt)
                })
                .collect::<LakeCatResult<Vec<_>>>()?
                .into_iter(),
        )?;
        let receipt = ViewVersionReceipt::drop(&record, previous_receipt_hash, principal)?;
        state
            .view_version_receipts
            .push(MemoryViewVersionReceipt { view_key, receipt });
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
            .iter()
            .map(|(view_key, view)| {
                validate_view_record_map_scope(view, view_key)?;
                Ok(view)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
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
        let profiles = state
            .storage_profiles
            .iter()
            .map(|(profile_key, profile)| {
                validate_storage_profile_map_scope(profile, profile_key)?;
                Ok(profile)
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
        Ok(storage_profile_match(profiles.into_iter(), table)?
            .unwrap_or_else(|| StorageProfile::inferred_for_table(table)))
    }

    async fn upsert_policy_binding(&self, binding: PolicyBinding) -> LakeCatResult<PolicyBinding> {
        binding.validate()?;
        let mut state = self.state.write().await;
        let key = policy_binding_key(&binding.warehouse, &binding.policy_id);
        if let Some(existing) = state.policy_bindings.get(&key) {
            validate_policy_binding_map_scope(existing, &key)?;
        }
        state.policy_bindings.insert(key, binding.clone());
        Ok(binding)
    }

    async fn list_policy_bindings(
        &self,
        warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        let state = self.state.read().await;
        let mut bindings = state
            .policy_bindings
            .iter()
            .map(|(binding_key, binding)| {
                validate_policy_binding_map_scope(binding, binding_key)?;
                Ok(binding)
            })
            .collect::<LakeCatResult<Vec<_>>>()?
            .into_iter()
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
        let bindings = state
            .policy_bindings
            .iter()
            .map(|(binding_key, binding)| {
                validate_policy_binding_map_scope(binding, binding_key)?;
                Ok(binding)
            })
            .collect::<LakeCatResult<Vec<_>>>()?;
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
        for event in &state.outbox_events {
            if event.delivered_at.is_none() && event_ids.contains(&event.event_id) {
                event.validate_pending()?;
            }
        }
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

fn validate_table_record_identity(record: &TableRecord, ident: &TableIdent) -> LakeCatResult<()> {
    record.validate()?;
    if record.ident != *ident {
        return Err(LakeCatError::Internal(
            "table record row scope does not match requested table".to_string(),
        ));
    }
    Ok(())
}

fn validate_table_record_map_scope(record: &TableRecord, record_key: &str) -> LakeCatResult<()> {
    record.validate()?;
    if table_key(&record.ident) != record_key {
        return Err(LakeCatError::Internal(
            "table record row scope does not match requested table".to_string(),
        ));
    }
    Ok(())
}

fn validate_soft_delete_record_map_scope(
    record: &SoftDeleteRecord,
    record_key: &str,
) -> LakeCatResult<()> {
    if table_key(&record.table) != record_key {
        return Err(LakeCatError::Internal(
            "soft-delete row scope does not match record identity".to_string(),
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

fn validate_idempotency_record_request_hash(row_request_hash: &str) -> LakeCatResult<()> {
    validate_idempotency_request_hash_shape(row_request_hash).map_err(|_| {
        LakeCatError::Internal(
            "idempotency record request hash must be full SHA-256 evidence".to_string(),
        )
    })
}

fn validate_table_commit_record_memory_scope(
    commit: &MemoryCommitRecord,
    ident: &TableIdent,
) -> LakeCatResult<()> {
    if commit.table_key != table_key(ident) {
        return Err(LakeCatError::Internal(
            "table commit record row scope does not match requested table".to_string(),
        ));
    }
    commit.record.validate_for_table(ident)
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

fn validate_policy_binding_map_scope(
    binding: &PolicyBinding,
    binding_key: &str,
) -> LakeCatResult<()> {
    binding.validate()?;
    if policy_binding_key(&binding.warehouse, &binding.policy_id) != binding_key {
        return Err(LakeCatError::Internal(
            "policy binding row scope does not match binding identity".to_string(),
        ));
    }
    Ok(())
}

fn validate_storage_profile_map_scope(
    profile: &StorageProfile,
    profile_key: &str,
) -> LakeCatResult<()> {
    profile.validate()?;
    if storage_profile_key(&profile.warehouse, &profile.profile_id) != profile_key {
        return Err(LakeCatError::Internal(
            "storage profile row scope does not match profile identity".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "turso-local")]
fn validate_storage_profile_scope(
    profile: &StorageProfile,
    warehouse: &WarehouseName,
    profile_key: &str,
    profile_id: &str,
    row_location_prefix: &str,
    row_provider: &str,
    row_issuance_mode: &str,
) -> LakeCatResult<()> {
    profile.validate()?;
    if profile.warehouse != *warehouse
        || storage_profile_key(warehouse, profile_id) != profile_key
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

fn validate_server_record_map_scope(server: &ServerRecord, server_id: &str) -> LakeCatResult<()> {
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

fn validate_project_record_map_scope(
    project: &ProjectRecord,
    project_id: &str,
) -> LakeCatResult<()> {
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

fn validate_warehouse_record_map_scope(
    record: &WarehouseRecord,
    warehouse_key: &str,
) -> LakeCatResult<()> {
    record.validate()?;
    if record.warehouse.as_str() != warehouse_key {
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

fn validate_secret_ref_issuance_mode(
    secret_ref: Option<&str>,
    issuance_mode: CredentialIssuanceMode,
) -> LakeCatResult<()> {
    if let Some(secret_ref) = secret_ref
        && !matches!(issuance_mode, CredentialIssuanceMode::ShortLivedSecretRef)
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "storage profile secret reference requires short-lived-secret-ref issuance mode; {}",
            secret_ref_hash_context(secret_ref)
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
            "policy id contains unsupported characters; {}",
            policy_id_hash_context(policy_id)
        )));
    }
    Ok(())
}

fn policy_id_hash_context(policy_id: &str) -> String {
    format!(
        "policy-id-hash={}",
        content_hash_bytes(policy_id.as_bytes())
    )
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
mod memory_tests;

#[cfg(feature = "turso-local")]
pub mod turso_store;
