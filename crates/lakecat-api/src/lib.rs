use lakecat_core::{LakeCatError, LakeCatResult, Namespace, TableIdent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct CatalogConfigResponse {
    pub defaults: Vec<ConfigEntry>,
    pub overrides: Vec<ConfigEntry>,
    pub endpoints: Vec<String>,
}

impl Default for CatalogConfigResponse {
    fn default() -> Self {
        Self {
            defaults: vec![
                ConfigEntry::new("lakecat.compatibility", "iceberg-rest"),
                ConfigEntry::new("lakecat.format.baseline", "iceberg-v1-v3"),
                ConfigEntry::new("lakecat.format.v4", "extension-ready"),
            ],
            overrides: Vec::new(),
            endpoints: vec![
                "GET /catalog/v1/config".to_string(),
                "GET /catalog/v1/namespaces".to_string(),
                "POST /catalog/v1/namespaces".to_string(),
                "GET /catalog/v1/namespaces/{namespace}/tables/{table}".to_string(),
                "POST /catalog/v1/namespaces/{namespace}/tables/{table}/commit".to_string(),
                "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan".to_string(),
                "POST /catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks"
                    .to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

impl ConfigEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateNamespaceRequest {
    pub namespace: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamespaceResponse {
    pub namespace: Vec<String>,
    pub properties: Vec<ConfigEntry>,
}

impl NamespaceResponse {
    pub fn from_namespace(namespace: &Namespace) -> Self {
        Self {
            namespace: namespace.parts().to_vec(),
            properties: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListNamespacesResponse {
    pub namespaces: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CreateTableRequest {
    pub name: String,
    pub location: String,
    pub metadata_location: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct LoadTableResponse {
    pub identifier: TableIdentifier,
    pub metadata_location: Option<String>,
    pub metadata: Value,
    pub config: Vec<ConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableIdentifier {
    pub namespace: Vec<String>,
    pub name: String,
}

impl TableIdentifier {
    pub fn from_ident(ident: &TableIdent) -> Self {
        Self {
            namespace: ident.namespace.parts().to_vec(),
            name: ident.name.as_str().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitTableRequest {
    #[serde(default)]
    pub requirements: Vec<Value>,
    #[serde(default)]
    pub updates: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CommitTableResponse {
    pub metadata_location: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PlanTableScanRequest {
    #[serde(default)]
    pub projection: Vec<String>,
    #[serde(default)]
    pub select: Vec<String>,
    #[serde(default)]
    pub filters: Vec<Value>,
    pub filter: Option<Value>,
    pub limit: Option<u64>,
    pub snapshot_id: Option<i64>,
    pub case_sensitive: Option<bool>,
    pub use_snapshot_schema: Option<bool>,
    pub start_snapshot_id: Option<i64>,
    pub end_snapshot_id: Option<i64>,
    #[serde(default)]
    pub stats_fields: Vec<String>,
}

impl PlanTableScanRequest {
    pub fn validate_scan_mode(&self) -> LakeCatResult<()> {
        if self.snapshot_id.is_some() && self.is_incremental_scan() {
            return Err(LakeCatError::InvalidArgument(
                "Iceberg scan planning cannot mix snapshot-id with incremental start/end snapshot ids"
                    .to_string(),
            ));
        }
        if self.start_snapshot_id.is_some() && self.end_snapshot_id.is_none() {
            return Err(LakeCatError::InvalidArgument(
                "Iceberg incremental scan planning requires end-snapshot-id when start-snapshot-id is set"
                    .to_string(),
            ));
        }
        if self.end_snapshot_id.is_some() && self.start_snapshot_id.is_none() {
            return Err(LakeCatError::InvalidArgument(
                "Iceberg incremental scan planning requires start-snapshot-id when end-snapshot-id is set"
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub fn is_incremental_scan(&self) -> bool {
        self.start_snapshot_id.is_some() || self.end_snapshot_id.is_some()
    }

    pub fn projected_fields(&self) -> Vec<String> {
        if self.select.is_empty() {
            self.projection.clone()
        } else {
            self.select.clone()
        }
    }

    pub fn filter_values(&self) -> Vec<Value> {
        let mut filters = self.filters.clone();
        if let Some(filter) = &self.filter {
            filters.push(filter.clone());
        }
        filters
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PlanTableScanResponse {
    pub table: TableIdentifier,
    pub planned_by: String,
    pub status: String,
    pub snapshot_id: Option<i64>,
    pub plan_tasks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lakecat_plan_tasks: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_scan_tasks: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delete_files: Vec<Value>,
    pub residual_filter: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct FetchScanTasksRequest {
    pub plan_task: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct FetchScanTasksResponse {
    pub table: TableIdentifier,
    pub planned_by: String,
    pub plan_task: String,
    pub snapshot_id: Option<i64>,
    pub file_scan_tasks: Vec<Value>,
    pub delete_files: Vec<Value>,
    pub plan_tasks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lakecat_plan_tasks: Vec<Value>,
    pub residual_filter: Option<Value>,
}
