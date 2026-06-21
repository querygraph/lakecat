use lakecat_core::{LakeCatError, LakeCatResult, Namespace, TableIdent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
                "GET /catalog/v1/namespaces/{namespace}".to_string(),
                "DELETE /catalog/v1/namespaces/{namespace}".to_string(),
                "GET /catalog/v1/namespaces/{namespace}/tables/{table}".to_string(),
                "DELETE /catalog/v1/namespaces/{namespace}/tables/{table}".to_string(),
                "POST /catalog/v1/namespaces/{namespace}/tables/{table}/commit".to_string(),
                "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan".to_string(),
                "POST /catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks"
                    .to_string(),
                "GET /catalog/v1/namespaces/{namespace}/tables/{table}/credentials".to_string(),
                "GET /catalog/v1/{warehouse}/config".to_string(),
                "GET /catalog/v1/{warehouse}/namespaces".to_string(),
                "POST /catalog/v1/{warehouse}/namespaces".to_string(),
                "GET /catalog/v1/{warehouse}/namespaces/{namespace}".to_string(),
                "DELETE /catalog/v1/{warehouse}/namespaces/{namespace}".to_string(),
                "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}".to_string(),
                "DELETE /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}"
                    .to_string(),
                "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/commit"
                    .to_string(),
                "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/plan"
                    .to_string(),
                "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/fetch-scan-tasks"
                    .to_string(),
                "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/credentials"
                    .to_string(),
                "GET /catalog/v1/{warehouse}/namespaces/{namespace}/views".to_string(),
                "GET /catalog/v1/{warehouse}/namespaces/{namespace}/views/{view}".to_string(),
                "DELETE /catalog/v1/{warehouse}/namespaces/{namespace}/views/{view}".to_string(),
                "POST /catalog/v1/{warehouse}/namespaces/{namespace}/views/{view}".to_string(),
                "PUT /catalog/v1/{warehouse}/namespaces/{namespace}/views/{view}".to_string(),
                "GET /management/v1/servers".to_string(),
                "PUT /management/v1/servers/{server}".to_string(),
                "GET /management/v1/projects".to_string(),
                "PUT /management/v1/projects/{project}".to_string(),
                "GET /management/v1/projects/{project}/warehouses".to_string(),
                "PUT /management/v1/projects/{project}/warehouses/{warehouse}".to_string(),
                "GET /management/v1/warehouses".to_string(),
                "PUT /management/v1/warehouses/{warehouse}".to_string(),
                "POST /management/v1/warehouses/{warehouse}/namespaces/{namespace}/tables/{table}/restore"
                    .to_string(),
                "GET /management/v1/warehouses/{warehouse}/namespaces/{namespace}/tables/{table}/commits"
                    .to_string(),
                "GET /management/v1/warehouses/{warehouse}/storage-profiles".to_string(),
                "PUT /management/v1/warehouses/{warehouse}/storage-profiles/{profile}".to_string(),
                "GET /management/v1/warehouses/{warehouse}/namespaces/{namespace}/views"
                    .to_string(),
                "PUT /management/v1/warehouses/{warehouse}/namespaces/{namespace}/views/{view}"
                    .to_string(),
                "GET /management/v1/warehouses/{warehouse}/namespaces/{namespace}/views/{view}/version-receipts"
                    .to_string(),
                "GET /management/v1/warehouses/{warehouse}/namespaces/{namespace}/view-version-receipt-chains"
                    .to_string(),
                "DELETE /management/v1/warehouses/{warehouse}/namespaces/{namespace}/views/{view}"
                    .to_string(),
                "GET /management/v1/warehouses/{warehouse}/policies".to_string(),
                "PUT /management/v1/warehouses/{warehouse}/policies/{policy}".to_string(),
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
#[serde(rename_all = "kebab-case")]
pub struct LoadCredentialsResponse {
    pub storage_credentials: Vec<StorageCredential>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct UpsertServerRequest {
    pub display_name: Option<String>,
    pub endpoint_url: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ServerResponse {
    pub server_id: String,
    pub display_name: Option<String>,
    pub endpoint_url: Option<String>,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ListServersResponse {
    pub servers: Vec<ServerResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct UpsertProjectRequest {
    pub server_id: Option<String>,
    pub display_name: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectResponse {
    pub project_id: String,
    pub server_id: Option<String>,
    pub display_name: Option<String>,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ListProjectsResponse {
    pub projects: Vec<ProjectResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct UpsertWarehouseRequest {
    pub project_id: Option<String>,
    pub storage_root: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct WarehouseResponse {
    pub warehouse: String,
    pub project_id: String,
    pub storage_root: Option<String>,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ListWarehousesResponse {
    pub warehouses: Vec<WarehouseResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct StorageCredential {
    pub prefix: String,
    pub config: Vec<ConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct UpsertStorageProfileRequest {
    pub location_prefix: String,
    pub provider: String,
    pub issuance_mode: String,
    pub secret_ref: Option<String>,
    #[serde(default)]
    pub public_config: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct StorageProfileResponse {
    pub profile_id: String,
    pub warehouse: String,
    pub location_prefix: String,
    pub provider: String,
    pub issuance_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<String>,
    pub secret_ref_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_ref_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_ref_hash: Option<String>,
    pub public_config: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ListStorageProfilesResponse {
    pub storage_profiles: Vec<StorageProfileResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct UpsertViewRequest {
    pub sql: String,
    #[serde(default = "default_sql_dialect")]
    pub dialect: String,
    #[serde(default)]
    pub schema_version: Option<u64>,
    #[serde(default)]
    pub expected_view_version: Option<u64>,
    #[serde(default)]
    pub columns: Vec<ViewColumnRequest>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ViewColumnRequest {
    pub name: String,
    pub data_type: Value,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub comment: Option<String>,
}

fn default_sql_dialect() -> String {
    "sql".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ViewResponse {
    pub warehouse: String,
    pub namespace: Vec<String>,
    pub name: String,
    pub view_version: u64,
    pub sql: String,
    pub dialect: String,
    pub schema_version: Option<u64>,
    pub columns: Vec<ViewColumnResponse>,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ViewColumnResponse {
    pub name: String,
    pub data_type: Value,
    pub nullable: bool,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ListViewsResponse {
    pub views: Vec<ViewResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ViewVersionReceiptResponse {
    pub stable_id: String,
    pub warehouse: String,
    pub namespace: Vec<String>,
    pub name: String,
    pub view_version: u64,
    pub previous_view_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_receipt_hash: Option<String>,
    pub operation: String,
    pub view_hash: String,
    pub receipt_hash: String,
    pub principal_subject: String,
    pub principal_kind: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ListViewVersionReceiptsResponse {
    pub receipts: Vec<ViewVersionReceiptResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ViewVersionReceiptChainResponse {
    pub stable_id: String,
    pub warehouse: String,
    pub namespace: Vec<String>,
    pub name: String,
    pub chain_hash: String,
    pub chain_verified: bool,
    pub latest_view_version: u64,
    pub latest_operation: String,
    pub tombstoned: bool,
    pub receipt_count: usize,
    pub receipts: Vec<ViewVersionReceiptResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ListViewVersionReceiptChainsResponse {
    pub chains: Vec<ViewVersionReceiptChainResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct UpsertPolicyBindingRequest {
    pub namespace: Option<Vec<String>>,
    pub table: Option<String>,
    #[serde(default = "default_enforced")]
    pub enforced: bool,
    #[serde(default)]
    pub odrl: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PolicyBindingResponse {
    pub policy_id: String,
    pub warehouse: String,
    pub namespace: Option<Vec<String>>,
    pub table: Option<String>,
    pub enforced: bool,
    pub odrl: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct ListPolicyBindingsResponse {
    pub policies: Vec<PolicyBindingResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct LineageDrainResponse {
    pub delivered: usize,
    pub event_types: Vec<String>,
    pub graph_events: usize,
    pub lineage_events: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_receipt_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_identity_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_identity_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typedid_envelope_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typedid_proof_hash: Option<String>,
    #[serde(default)]
    pub events: Vec<LineageDrainEventSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct LineageDrainEventSummary {
    pub event_id: String,
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_receipt_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_identity_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_identity_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typedid_envelope_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typedid_proof_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_delegation_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_summary_signature_hash: Option<String>,
    #[serde(default)]
    pub graph_events: usize,
    #[serde(default)]
    pub lineage_events: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_lineage_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub querygraph_import_hash: Option<String>,
    #[serde(default)]
    pub table_artifact_count: usize,
    #[serde(default)]
    pub view_artifact_count: usize,
    #[serde(default)]
    pub view_version_receipt_hashes: Vec<String>,
    #[serde(default)]
    pub view_version_receipt_chain_hashes: Vec<String>,
    #[serde(default)]
    pub view_version_receipt_chain_verified_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub view_version_receipt_chains: Vec<ViewVersionReceiptChainResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_warehouse: Option<String>,
    #[serde(default)]
    pub view_namespace: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_stable_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_view_version: Option<u64>,
    #[serde(default)]
    pub policy_binding_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub server_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_profile_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub storage_profile_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_profile_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_profile_issuance_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_profile_location_prefix_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_profile_secret_ref_present: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_profile_secret_ref_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_profile_secret_ref_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warehouse_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warehouse_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_commit_count: Option<usize>,
    #[serde(default)]
    pub table_commit_sequence_numbers: Vec<u64>,
    #[serde(default)]
    pub table_commit_hashes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_task_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_scan_task_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_file_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_plan_task_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_restriction: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_projection: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_projection: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effective_projection: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_filters: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_stats_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effective_stats_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub management_scope_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub management_scope_warehouse: Option<String>,
    #[serde(default)]
    pub standards: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_block_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_credential_exception_allowed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_credential_exception_reason: Option<String>,
    #[serde(default)]
    pub replay_event_hashes: Vec<String>,
    #[serde(default)]
    pub replay_open_lineage_hashes: Vec<String>,
}

fn default_enforced() -> bool {
    true
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
#[serde(rename_all = "kebab-case")]
pub struct CommitTableRequest {
    #[serde(default)]
    pub requirements: Vec<Value>,
    #[serde(default)]
    pub updates: Vec<Value>,
    pub metadata_location: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CommitTableResponse {
    pub metadata_location: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct TableCommitRecordResponse {
    pub warehouse: String,
    pub namespace: Vec<String>,
    pub table: String,
    pub previous_metadata_location: Option<String>,
    pub new_metadata_location: Option<String>,
    pub sequence_number: u64,
    pub format_version: Option<i32>,
    pub snapshot_id: Option<i64>,
    pub policy_hash: Option<String>,
    pub request_hash: String,
    pub response_hash: String,
    pub idempotency_key_sha256: Option<String>,
    pub commit_hash: String,
    pub principal_subject: String,
    pub principal_kind: String,
    pub committed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ListTableCommitRecordsResponse {
    pub commits: Vec<TableCommitRecordResponse>,
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
