use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use lakecat_core::{
    AuditStamp, LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, TableName,
    WarehouseName, content_hash_bytes, content_hash_json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

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

pub(crate) fn validate_idempotency_key_shape(value: &str) -> LakeCatResult<()> {
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

pub(crate) fn validate_idempotency_request_hash_shape(value: &str) -> LakeCatResult<()> {
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

pub(crate) fn validate_outbox_event_id_shape(value: &str) -> LakeCatResult<()> {
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

pub(crate) fn validate_commit_record_metadata_location(
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

pub(crate) fn validate_sha256_evidence(value: &str, message: &str) -> LakeCatResult<()> {
    let Some(digest) = value.strip_prefix("sha256:") else {
        return Err(LakeCatError::Internal(message.to_string()));
    };
    if digest.len() != 64 || !digest.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(LakeCatError::Internal(message.to_string()));
    }
    Ok(())
}

pub(crate) fn table_response_hash(table: &TableRecord) -> LakeCatResult<String> {
    let value = serde_json::to_value(table).map_err(|err| {
        LakeCatError::Internal(format!(
            "failed to encode table commit response hash: {err}"
        ))
    })?;
    content_hash_json(&value)
}

pub(crate) fn validate_table_metadata_format_version(
    metadata: &Value,
    label: &str,
) -> LakeCatResult<()> {
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

pub(crate) fn table_metadata_format_version(metadata: &Value) -> Option<i32> {
    metadata
        .get("format-version")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

pub(crate) fn table_commit_format_version(table: &TableRecord) -> Option<i32> {
    table_metadata_format_version(&table.metadata)
}

pub(crate) fn table_commit_snapshot_id(table: &TableRecord) -> Option<i64> {
    Some(
        table
            .metadata
            .get("current-snapshot-id")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    )
}

pub(crate) fn table_commit_policy_hash(authorization_receipt: Option<&Value>) -> Option<String> {
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

pub(crate) fn default_view_version() -> u64 {
    1
}

pub(crate) fn validate_expected_view_version(expected: u64) -> LakeCatResult<()> {
    if expected == 0 {
        return Err(LakeCatError::InvalidArgument(
            "expected view version must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn require_expected_view_version(
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
    pub(crate) fn upsert(
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

    pub(crate) fn drop(
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

pub(crate) fn view_receipt_hash(receipt: &ViewVersionReceipt) -> LakeCatResult<String> {
    content_hash_json(&serde_json::to_value(receipt).map_err(|err| {
        LakeCatError::Internal(format!("failed to serialize view receipt: {err}"))
    })?)
}

pub(crate) fn latest_view_receipt_evidence<'a>(
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

pub(crate) fn latest_view_receipt_hash<'a>(
    receipts: impl Iterator<Item = &'a ViewVersionReceipt>,
) -> LakeCatResult<Option<String>> {
    latest_view_receipt_evidence(receipts).map(|evidence| evidence.map(|(_, hash)| hash))
}

pub(crate) fn validate_memory_view_receipt_scope(
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

pub(crate) fn memory_view_receipt_key_matches_namespace(
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

pub(crate) fn validate_view_receipt_chains(receipts: &[ViewVersionReceipt]) -> LakeCatResult<()> {
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
pub(crate) fn validate_view_receipt_scope(
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

pub(crate) fn validate_view_record_scope(
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

pub(crate) fn validate_view_record_map_scope(
    record: &ViewRecord,
    record_key: &str,
) -> LakeCatResult<()> {
    record.validate()?;
    if view_key(record) != record_key {
        return Err(LakeCatError::Internal(
            "view record row scope does not match view identity".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn view_version_operation_order(operation: &ViewVersionOperation) -> u8 {
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
