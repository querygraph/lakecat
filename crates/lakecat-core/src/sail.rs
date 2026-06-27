// Sail catalog engine seam: trait + request/response types + deferred
// (no-op) implementation. These live in lakecat-core so lakecat-service
// can depend on them without pulling in lakecat-sail's local Sail path
// deps. lakecat-sail re-exports everything here and adds the real
// sail-local and catalog-provider implementations behind feature flags.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{LakeCatError, LakeCatResult, Principal, TableIdent};

#[async_trait]
pub trait SailCatalogEngine: Send + Sync + 'static {
    async fn prepare_commit(&self, request: CommitPreparationRequest) -> LakeCatResult<CommitPlan>;
    async fn plan_scan(&self, request: ScanPlanningRequest) -> LakeCatResult<ScanPlan>;
    async fn fetch_scan_tasks(
        &self,
        request: FetchScanTasksRequest,
    ) -> LakeCatResult<FetchScanTasksPlan>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitPreparationRequest {
    pub table: TableIdent,
    pub principal: Principal,
    pub current_metadata_location: Option<String>,
    pub new_metadata_location: Option<String>,
    pub current_metadata: Value,
    pub new_metadata: Option<Value>,
    pub requirements: Vec<Value>,
    pub updates: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitPlan {
    pub prepared_by: String,
    pub requirements: Vec<Value>,
    pub updates: Vec<Value>,
    pub new_metadata_location: Option<String>,
    pub new_metadata: Value,
    pub metadata_write_required: bool,
    pub metadata_patch: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanPlanningRequest {
    pub table: TableIdent,
    pub principal: Principal,
    pub metadata_location: Option<String>,
    pub table_metadata: Value,
    pub projection: Vec<String>,
    pub filters: Vec<Value>,
    pub limit: Option<u64>,
    pub snapshot_id: Option<i64>,
    pub start_snapshot_id: Option<i64>,
    pub end_snapshot_id: Option<i64>,
}

impl ScanPlanningRequest {
    pub fn is_incremental_scan(&self) -> bool {
        self.start_snapshot_id.is_some() || self.end_snapshot_id.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanPlan {
    pub planned_by: String,
    pub snapshot_id: Option<i64>,
    pub scan_tasks: Vec<Value>,
    pub residual_filter: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FetchScanTasksRequest {
    pub table: TableIdent,
    pub principal: Principal,
    pub metadata_location: Option<String>,
    pub table_metadata: Value,
    pub plan_task: String,
    #[serde(default)]
    pub required_projection: Vec<String>,
    #[serde(default)]
    pub required_filters: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FetchScanTasksPlan {
    pub planned_by: String,
    pub plan_task: String,
    pub snapshot_id: Option<i64>,
    pub file_scan_tasks: Vec<Value>,
    pub delete_files: Vec<Value>,
    pub plan_tasks: Vec<Value>,
    pub residual_filter: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SailMetadataSummary {
    pub format_version: i32,
    pub table_uuid: Option<String>,
    pub table_location: Option<String>,
    pub current_schema_id: Option<i32>,
    pub current_snapshot_id: Option<i64>,
    pub sequence_number: Option<i64>,
    pub last_assigned_field_id: Option<i32>,
    pub last_assigned_partition_id: Option<i32>,
    pub default_spec_id: Option<i32>,
    pub default_sort_order_id: Option<i64>,
    pub manifest_list: Option<String>,
    pub v4_extension_mode: bool,
    pub fields: Vec<SailFieldSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SailFieldSummary {
    pub id: i32,
    pub name: String,
    pub data_type: String,
    pub required: bool,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SailScanTask {
    pub task_type: String,
    pub table: String,
    pub snapshot_id: i64,
    pub plan_task: String,
    pub metadata_location: Option<String>,
    pub manifest_list: Option<String>,
    pub manifest_path: Option<String>,
    pub content: Option<String>,
    pub sequence_number: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SailFilterSummary {
    pub expression_type: String,
    pub references: Vec<String>,
    pub filter: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IcebergFormatSupport {
    pub stable_versions: Vec<i32>,
    pub v4_ready: bool,
}

impl Default for IcebergFormatSupport {
    fn default() -> Self {
        Self {
            stable_versions: vec![1, 2, 3],
            v4_ready: true,
        }
    }
}

/// No-op Sail engine used when the `sail-local` feature is not enabled.
/// Commit preparation passes through; scan planning returns `NotSupported`.
#[derive(Debug, Default)]
pub struct DeferredSailCatalogEngine;

impl DeferredSailCatalogEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl SailCatalogEngine for DeferredSailCatalogEngine {
    async fn prepare_commit(&self, request: CommitPreparationRequest) -> LakeCatResult<CommitPlan> {
        // The deferred engine cannot evolve table metadata by Iceberg `updates`
        // (applying add-snapshot / schema / spec changes is Sail table-format work,
        // available with the `sail-local` feature). When a commit carries updates
        // but no replacement metadata document, the catalog is expected to apply
        // them server-side — which this build cannot do. Reject honestly with
        // `NotSupported` rather than silently returning the table unchanged, which
        // looks like a successful commit while dropping every update. Register-style
        // commits that carry the full `new_metadata` are still served, and a commit
        // with no updates is a no-op pass-through.
        if request.new_metadata.is_none() && !request.updates.is_empty() {
            return Err(LakeCatError::NotSupported(format!(
                "applying {} Iceberg table update(s) server-side requires the \
                 `sail-local` build; this deferred build accepts only commits that \
                 carry the full replacement table metadata",
                request.updates.len()
            )));
        }
        let metadata_write_required =
            request.new_metadata_location.is_some() || request.new_metadata.is_some();
        let new_metadata_location = request
            .new_metadata_location
            .clone()
            .or_else(|| request.current_metadata_location.clone());
        let new_metadata = request.new_metadata.unwrap_or(request.current_metadata);
        Ok(CommitPlan {
            prepared_by: "lakecat-sail-deferred".to_string(),
            requirements: request.requirements,
            updates: request.updates,
            new_metadata_location,
            new_metadata,
            metadata_write_required,
            metadata_patch: serde_json::json!({
                "lakecat:sail-delegation": "deferred",
                "lakecat:sail-target": "sail-catalog + sail-iceberg",
                "lakecat:format-support": IcebergFormatSupport::default(),
            }),
        })
    }

    async fn plan_scan(&self, request: ScanPlanningRequest) -> LakeCatResult<ScanPlan> {
        Err(LakeCatError::NotSupported(format!(
            "Sail scan planning is unavailable without the `sail-local` feature for {}",
            request.table.stable_id()
        )))
    }

    async fn fetch_scan_tasks(
        &self,
        request: FetchScanTasksRequest,
    ) -> LakeCatResult<FetchScanTasksPlan> {
        Err(LakeCatError::NotSupported(format!(
            "Sail fetchScanTasks is not wired yet for {}",
            request.table.stable_id()
        )))
    }
}

/// Synthesize an initial, empty Iceberg table metadata document from a
/// standard `createTable` request (a schema, optional partition spec / sort
/// order / properties). The Iceberg REST spec has the catalog create table
/// metadata server-side; full typed construction belongs in Sail, but the
/// catalog needs a minimal compliant document so a stock client can create a
/// table without supplying metadata itself. The result is a valid empty table
/// (no snapshots) that ordinary engines and LakeCat's own validators accept.
pub fn initial_table_metadata(
    table_uuid: &str,
    location: &str,
    schema: &Value,
    partition_spec: Option<&Value>,
    sort_order: Option<&Value>,
    properties: &Value,
) -> Value {
    // schema-id and field ids come from the client schema; derive last-column-id
    // as the max field id so later column additions keep increasing it.
    let mut schema = schema.clone();
    if schema.get("schema-id").is_none() {
        if let Some(obj) = schema.as_object_mut() {
            obj.insert("schema-id".into(), Value::from(0));
        }
    }
    let current_schema_id = schema.get("schema-id").and_then(Value::as_i64).unwrap_or(0);
    let last_column_id = max_field_id(&schema);

    let partition_spec = partition_spec
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"spec-id": 0, "fields": []}));
    let default_spec_id = partition_spec
        .get("spec-id")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let sort_order = sort_order
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"order-id": 0, "fields": []}));
    let default_sort_order_id = sort_order
        .get("order-id")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let now_ms = chrono::Utc::now().timestamp_millis();

    serde_json::json!({
        "format-version": 2,
        "table-uuid": table_uuid,
        "location": location,
        "last-sequence-number": 0,
        "last-updated-ms": now_ms,
        "last-column-id": last_column_id,
        "schemas": [schema],
        "current-schema-id": current_schema_id,
        "partition-specs": [partition_spec],
        "default-spec-id": default_spec_id,
        "last-partition-id": 999,
        "sort-orders": [sort_order],
        "default-sort-order-id": default_sort_order_id,
        "properties": if properties.is_object() { properties.clone() } else { serde_json::json!({}) },
        "current-snapshot-id": -1,
        "snapshots": [],
        "snapshot-log": [],
        "metadata-log": []
    })
}

fn max_field_id(schema: &Value) -> i64 {
    fn walk(v: &Value, max: &mut i64) {
        match v {
            Value::Object(map) => {
                if let Some(id) = map.get("id").and_then(Value::as_i64) {
                    if id > *max {
                        *max = id;
                    }
                }
                for (_k, child) in map {
                    walk(child, max);
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, max);
                }
            }
            _ => {}
        }
    }
    let mut max = 0;
    walk(schema, &mut max);
    max
}

pub fn validate_lakecat_metadata_format(metadata: &Value) -> LakeCatResult<IcebergFormatSupport> {
    let support = IcebergFormatSupport::default();
    let Some(version) = metadata.get("format-version").and_then(Value::as_i64) else {
        return Ok(support);
    };
    if support.stable_versions.contains(&(version as i32)) {
        return Ok(support);
    }
    if version == 4 && support.v4_ready {
        return Ok(support);
    }
    Err(LakeCatError::NotSupported(format!(
        "unsupported Iceberg table format version v{version}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Namespace, TableName, WarehouseName};

    #[tokio::test]
    async fn deferred_engine_rejects_scan_planning() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let error = DeferredSailCatalogEngine::new()
            .plan_scan(ScanPlanningRequest {
                table,
                principal: Principal::anonymous(),
                metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
                table_metadata: serde_json::json!({"format-version": 3}),
                projection: Vec::new(),
                filters: Vec::new(),
                limit: None,
                snapshot_id: None,
                start_snapshot_id: None,
                end_snapshot_id: None,
            })
            .await
            .expect_err("deferred engine must not claim a successful empty scan plan");

        assert!(matches!(error, LakeCatError::NotSupported(_)));
        assert!(error.to_string().contains("sail-local"));
    }

    fn commit_request(
        updates: Vec<serde_json::Value>,
        new_metadata: Option<serde_json::Value>,
    ) -> CommitPreparationRequest {
        CommitPreparationRequest {
            table: TableIdent::new(
                WarehouseName::new("local").unwrap(),
                "default".parse::<Namespace>().unwrap(),
                TableName::new("events").unwrap(),
            ),
            principal: Principal::anonymous(),
            current_metadata_location: Some(
                "file:///tmp/events/metadata/00000.json".to_string(),
            ),
            new_metadata_location: None,
            current_metadata: serde_json::json!({"format-version": 2, "snapshots": []}),
            new_metadata,
            requirements: Vec::new(),
            updates,
        }
    }

    #[tokio::test]
    async fn deferred_engine_rejects_commit_with_unapplied_updates() {
        // The exact silent-drop case (finding H9): updates present, no replacement
        // metadata — the deferred build must reject, not return the table unchanged.
        let error = DeferredSailCatalogEngine::new()
            .prepare_commit(commit_request(
                vec![serde_json::json!({"action": "add-snapshot"})],
                None,
            ))
            .await
            .expect_err("deferred engine must not silently drop table updates");
        assert!(matches!(error, LakeCatError::NotSupported(_)));
        assert!(error.to_string().contains("sail-local"));
    }

    #[tokio::test]
    async fn deferred_engine_serves_register_style_commit() {
        // A commit carrying the full replacement metadata is still served.
        let new = serde_json::json!({"format-version": 2, "snapshots": [{"snapshot-id": 1}]});
        let plan = DeferredSailCatalogEngine::new()
            .prepare_commit(commit_request(
                vec![serde_json::json!({"action": "add-snapshot"})],
                Some(new.clone()),
            ))
            .await
            .expect("register-style commit with replacement metadata is served");
        assert_eq!(plan.new_metadata, new);
    }

    #[tokio::test]
    async fn deferred_engine_passes_through_no_update_commit() {
        // An empty-update commit is a no-op pass-through of the current metadata.
        let plan = DeferredSailCatalogEngine::new()
            .prepare_commit(commit_request(Vec::new(), None))
            .await
            .expect("empty-update commit is a no-op pass-through");
        assert_eq!(
            plan.new_metadata,
            serde_json::json!({"format-version": 2, "snapshots": []})
        );
    }
}
