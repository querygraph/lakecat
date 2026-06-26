use std::sync::Arc;

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use lakecat_core::{LakeCatError, LakeCatResult, TableIdent};
use object_store::local::LocalFileSystem;
use sail_catalog_iceberg::{
    completed_planning_with_id_result_from_values, fetch_scan_tasks_result_from_values, models,
};
use sail_iceberg::io::{StoreContext, load_manifest, load_manifest_list};
use sail_iceberg::spec::catalog::TableUpdate;
use sail_iceberg::spec::metadata::apply_table_updates;
use sail_iceberg::spec::{
    DataContentType, DataFile as SailDataFile, DataFileFormat, Datum, DeleteFileIndex,
    DeleteFileRef, Literal, MAIN_BRANCH, ManifestContentType, ManifestStatus, PrimitiveLiteral,
    Snapshot, TableMetadata, TableRequirement,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use url::Url;

use crate::{
    CommitPlan, CommitPreparationRequest, FetchScanTasksPlan, FetchScanTasksRequest,
    IcebergFormatSupport, SailCatalogEngine, SailFieldSummary, SailFilterSummary,
    SailMetadataSummary, SailScanTask, ScanPlan, ScanPlanningRequest,
    validate_lakecat_metadata_format,
};

#[derive(Debug, Default)]
pub struct SailRestModelCatalogEngine;

type HmacSha256 = Hmac<Sha256>;
const PLAN_TASK_SIGNING_KEY_ENV: &str = "LAKECAT_PLAN_TASK_SIGNING_KEY";
const DEFAULT_PLAN_TASK_SIGNING_KEY: &[u8] = b"lakecat-local-plan-task-signing-key-v1";

impl SailRestModelCatalogEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl SailCatalogEngine for SailRestModelCatalogEngine {
    async fn prepare_commit(&self, request: CommitPreparationRequest) -> LakeCatResult<CommitPlan> {
        let (metadata_summary, typed_metadata) =
            inspect_sail_table_metadata_with_typed(&request.current_metadata)?;
        let validated_requirements = match typed_metadata.as_ref() {
            Some(metadata) => validate_stable_commit_requirements(metadata, &request.requirements)?,
            None => {
                validate_v4_extension_commit_requirements(&metadata_summary, &request.requirements)?
            }
        };
        let sail_request = json!({
            "requirements": request.requirements,
            "updates": request.updates,
        });
        // Keep raw JSON updates for v4/extension work, but prove the envelope is
        // compatible with Sail's generated REST catalog model.
        let _: models::CommitTableRequest =
            serde_json::from_value(sail_request.clone()).map_err(|err| {
                LakeCatError::InvalidArgument(format!(
                    "invalid Iceberg REST commit request for Sail: {err}"
                ))
            })?;
        let requirements = sail_request["requirements"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let updates = sail_request["updates"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        // Three commit shapes:
        //  1. register/stage: the client supplied a full new metadata doc.
        //  2. standard updateTable: apply the REST updates to the current
        //     metadata via Sail to produce the new metadata + a fresh
        //     metadata-location (the catalog then writes it and CAS-es).
        //  3. no-op: no updates and no supplied metadata -> pass through.
        let (new_metadata, new_metadata_location, metadata_write_required) =
            if let Some(client_metadata) = request.new_metadata.clone() {
                let location = request
                    .new_metadata_location
                    .clone()
                    .or_else(|| request.current_metadata_location.clone());
                (client_metadata, location, true)
            } else if !updates.is_empty() {
                let mut metadata = typed_metadata.clone().ok_or_else(|| {
                    LakeCatError::NotSupported(
                        "applying Iceberg REST updates needs typed Sail metadata".to_string(),
                    )
                })?;
                let typed_updates: Vec<TableUpdate> =
                    serde_json::from_value(Value::Array(updates.clone())).map_err(|err| {
                        LakeCatError::InvalidArgument(format!(
                            "invalid Iceberg REST table updates for Sail: {err}"
                        ))
                    })?;
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                apply_table_updates(&mut metadata, &typed_updates, now_ms)
                    .map_err(|err| LakeCatError::InvalidArgument(err.to_string()))?;
                let new_metadata: Value =
                    serde_json::from_slice(&metadata.to_json().map_err(|err| {
                        LakeCatError::Internal(format!(
                            "failed to serialize updated table metadata: {err}"
                        ))
                    })?)
                    .map_err(|err| LakeCatError::Internal(err.to_string()))?;
                let location = next_metadata_location(&metadata.location, now_ms);
                (new_metadata, Some(location), true)
            } else {
                let location = request
                    .new_metadata_location
                    .clone()
                    .or_else(|| request.current_metadata_location.clone());
                (request.current_metadata.clone(), location, false)
            };
        validate_lakecat_metadata_format(&new_metadata)?;
        Ok(CommitPlan {
            prepared_by: "sail-rest-models".to_string(),
            requirements,
            updates,
            new_metadata_location: new_metadata_location.clone(),
            new_metadata,
            metadata_write_required,
            metadata_patch: json!({
                "lakecat:sail-delegation": "sail-catalog-iceberg-rest-models",
                "lakecat:format-support": IcebergFormatSupport::default(),
                "lakecat:sail-metadata": metadata_summary,
                "lakecat:validated-requirements": validated_requirements,
                "previous-metadata-location": request.current_metadata_location,
                "new-metadata-location": new_metadata_location,
            }),
        })
    }

    async fn plan_scan(&self, request: ScanPlanningRequest) -> LakeCatResult<ScanPlan> {
        let (metadata_summary, typed_metadata) =
            inspect_sail_table_metadata_with_typed(&request.table_metadata)?;
        validate_projection(&metadata_summary, &request.projection)?;
        let validated_filters = validate_scan_filters(&metadata_summary, &request.filters)?;
        let combined_filter = combined_scan_filter(&request.filters);
        let metadata_location = request.metadata_location.clone().ok_or_else(|| {
            LakeCatError::NotSupported(
                "Sail scan planning needs an Iceberg metadata location".to_string(),
            )
        })?;
        let scan_tasks = match typed_metadata.as_ref() {
            Some(metadata) => {
                stable_manifest_plan_tasks(
                    metadata,
                    &metadata_summary,
                    &request,
                    Some(metadata_location.clone()),
                )
                .await?
            }
            None => {
                if request.is_incremental_scan() {
                    return Err(LakeCatError::NotSupported(
                        "Iceberg v4 extension incremental scan planning needs typed Sail metadata"
                            .to_string(),
                    ));
                }
                v4_extension_manifest_plan_tasks(
                    &request,
                    &metadata_summary,
                    Some(metadata_location.clone()),
                )?
            }
        };
        let planned_snapshot_id = scan_tasks
            .first()
            .and_then(|task| task.get("snapshot-id"))
            .and_then(Value::as_i64)
            .or(request.snapshot_id)
            .or(metadata_summary.current_snapshot_id);
        let plan_task_count = scan_tasks.len();
        let plan_task_tokens = plan_task_tokens_from_values(&scan_tasks)?;
        completed_planning_with_id_result_from_values(
            None,
            plan_task_tokens,
            Vec::new(),
            Vec::new(),
        )
        .map_err(iceberg_rest_model_error)?;
        let scan_request = json!({
            "snapshot-id": planned_snapshot_id,
            "select": (!request.projection.is_empty()).then_some(request.projection.clone()),
            "filter": combined_filter,
            "case-sensitive": true,
        });
        let sail_scan: models::PlanTableScanRequest = serde_json::from_value(scan_request)
            .map_err(|err| {
                LakeCatError::InvalidArgument(format!(
                    "invalid Iceberg REST scan-planning request for Sail: {err}"
                ))
            })?;
        Ok(ScanPlan {
            planned_by: "sail-rest-models".to_string(),
            snapshot_id: sail_scan.snapshot_id,
            scan_tasks,
            residual_filter: Some(json!({
                "metadata-location": metadata_location,
                "select": sail_scan.select,
                "filters-accepted-by-sail": validated_filters,
                "limit-deferred-to-sail": request.limit,
                "scan-mode": if request.is_incremental_scan() { "incremental" } else { "point-in-time" },
                "start-snapshot-id": request.start_snapshot_id,
                "end-snapshot-id": request.end_snapshot_id,
                "plan-task-count": plan_task_count,
                "sail-metadata": metadata_summary,
            })),
        })
    }

    async fn fetch_scan_tasks(
        &self,
        request: FetchScanTasksRequest,
    ) -> LakeCatResult<FetchScanTasksPlan> {
        let (metadata_summary, typed_metadata) =
            inspect_sail_table_metadata_with_typed(&request.table_metadata)?;
        let decoded = decode_plan_task(&request.plan_task)?;
        validate_decoded_plan_task(
            &request,
            &metadata_summary,
            typed_metadata.as_ref(),
            &decoded,
        )
        .await?;
        let task = decoded.to_scan_task(
            request.table.stable_id(),
            request.metadata_location.clone(),
            metadata_summary.sequence_number,
        );
        let expanded = expand_fetch_plan_task_with_sail_io(
            &decoded,
            &metadata_summary,
            typed_metadata.as_ref(),
            &request,
        )
        .await?;
        let (file_scan_tasks, delete_files, plan_tasks) = match expanded {
            Some(expanded) => (
                expanded.file_scan_tasks,
                expanded.delete_files,
                expanded.plan_tasks,
            ),
            None => (Vec::new(), Vec::new(), vec![scan_task_value(task)?]),
        };
        fetch_scan_tasks_result_from_values(
            plan_task_tokens_from_values(&plan_tasks)?,
            file_scan_tasks.clone(),
            delete_files.clone(),
        )
        .map_err(iceberg_rest_model_error)?;
        Ok(FetchScanTasksPlan {
            planned_by: "sail-rest-models".to_string(),
            plan_task: request.plan_task,
            snapshot_id: Some(decoded.snapshot_id),
            file_scan_tasks,
            delete_files,
            plan_tasks,
            residual_filter: Some(json!({
                "lakecat:sail-delegation": "sail-iceberg-manifest-reader",
                "lakecat:sail-target": match decoded.kind.as_str() {
                    "manifest-list" => "sail_iceberg::io::load_manifest_list",
                    "manifest" => "sail_iceberg::io::load_manifest",
                    _ => "sail_iceberg::io",
                },
                "metadata-location": request.metadata_location,
                "plan-task": decoded.raw,
                "task-kind": decoded.kind,
                "manifest-path": decoded.path,
                "projection": decoded.projection,
                "filters": decoded.filters,
                "sail-metadata": metadata_summary,
            })),
        })
    }
}

fn plan_task_tokens_from_values(tasks: &[Value]) -> LakeCatResult<Vec<String>> {
    tasks
        .iter()
        .map(|task| {
            task.get("plan-task")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .ok_or_else(|| {
                    LakeCatError::Internal(
                        "Sail-generated Iceberg plan task is missing plan-task".to_string(),
                    )
                })
        })
        .collect()
}

fn iceberg_rest_model_error(error: impl std::fmt::Display) -> LakeCatError {
    LakeCatError::Internal(format!(
        "Sail generated an invalid Iceberg REST planning payload: {error}"
    ))
}

/// Derive a fresh, unique metadata object location under the table's
/// `metadata/` directory: `<table-location>/metadata/<ms>-<uuid>.metadata.json`.
/// The timestamp keeps it ordered and the UUID keeps it unique under
/// concurrent commits, so it never collides with the current pointer.
fn next_metadata_location(table_location: &str, now_ms: i64) -> String {
    let base = table_location.trim_end_matches('/');
    let token = uuid::Uuid::new_v4();
    format!("{base}/metadata/{now_ms:020}-{token}.metadata.json")
}

pub fn validate_sail_table_metadata(metadata: &Value) -> LakeCatResult<IcebergFormatSupport> {
    inspect_sail_table_metadata(metadata).map(|_| IcebergFormatSupport::default())
}

pub fn inspect_sail_table_metadata(metadata: &Value) -> LakeCatResult<SailMetadataSummary> {
    inspect_sail_table_metadata_with_typed(metadata).map(|(summary, _)| summary)
}

fn inspect_sail_table_metadata_with_typed(
    metadata: &Value,
) -> LakeCatResult<(SailMetadataSummary, Option<TableMetadata>)> {
    validate_lakecat_metadata_format(metadata)?;
    let format_version = metadata
        .get("format-version")
        .and_then(Value::as_i64)
        .unwrap_or(2) as i32;
    if format_version == 4 {
        return Ok((inspect_v4_extension_metadata(metadata), None));
    }
    let bytes = serde_json::to_vec(metadata).map_err(|err| {
        LakeCatError::InvalidArgument(format!("invalid Iceberg table metadata JSON: {err}"))
    })?;
    let sail_metadata = TableMetadata::from_json(&bytes).map_err(|err| {
        LakeCatError::InvalidArgument(format!("invalid Sail Iceberg table metadata model: {err}"))
    })?;
    let schema = sail_metadata.current_schema();
    let snapshot = sail_metadata.current_snapshot();
    let summary = SailMetadataSummary {
        format_version: sail_metadata.format_version as i32,
        table_uuid: sail_metadata.table_uuid.map(|uuid| uuid.to_string()),
        table_location: Some(sail_metadata.location.clone()),
        current_schema_id: schema.map(|schema| schema.schema_id()),
        current_snapshot_id: snapshot.map(|snapshot| snapshot.snapshot_id()),
        sequence_number: snapshot.map(|snapshot| snapshot.sequence_number()),
        last_assigned_field_id: Some(sail_metadata.last_column_id),
        last_assigned_partition_id: Some(sail_metadata.last_partition_id),
        default_spec_id: Some(sail_metadata.default_spec_id),
        default_sort_order_id: Some(
            sail_metadata
                .default_sort_order_id
                .map(i64::from)
                .unwrap_or(0),
        ),
        manifest_list: snapshot
            .map(|snapshot| snapshot.manifest_list().to_string())
            .filter(|manifest_list| !manifest_list.is_empty()),
        v4_extension_mode: false,
        fields: schema
            .map(|schema| {
                schema
                    .fields()
                    .iter()
                    .map(|field| SailFieldSummary {
                        id: field.id,
                        name: field.name.clone(),
                        data_type: field.field_type.to_string(),
                        required: field.required,
                        doc: field.doc.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    Ok((summary, Some(sail_metadata)))
}

fn inspect_v4_extension_metadata(metadata: &Value) -> SailMetadataSummary {
    SailMetadataSummary {
        format_version: 4,
        table_uuid: metadata
            .get("table-uuid")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        table_location: metadata
            .get("location")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        current_schema_id: metadata
            .get("current-schema-id")
            .and_then(Value::as_i64)
            .map(|id| id as i32),
        current_snapshot_id: metadata.get("current-snapshot-id").and_then(Value::as_i64),
        sequence_number: metadata.get("last-sequence-number").and_then(Value::as_i64),
        last_assigned_field_id: metadata
            .get("last-column-id")
            .and_then(Value::as_i64)
            .map(|id| id as i32),
        last_assigned_partition_id: metadata
            .get("last-partition-id")
            .and_then(Value::as_i64)
            .map(|id| id as i32),
        default_spec_id: metadata
            .get("default-spec-id")
            .and_then(Value::as_i64)
            .map(|id| id as i32),
        default_sort_order_id: metadata
            .get("default-sort-order-id")
            .and_then(Value::as_i64)
            .or(Some(0)),
        manifest_list: metadata
            .get("snapshots")
            .and_then(Value::as_array)
            .and_then(|snapshots| {
                let current_snapshot_id =
                    metadata.get("current-snapshot-id").and_then(Value::as_i64);
                snapshots.iter().find(|snapshot| {
                    snapshot.get("snapshot-id").and_then(Value::as_i64) == current_snapshot_id
                })
            })
            .and_then(|snapshot| snapshot.get("manifest-list"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        v4_extension_mode: true,
        fields: iceberg_json_fields(metadata),
    }
}

fn validate_stable_commit_requirements(
    metadata: &TableMetadata,
    requirements: &[Value],
) -> LakeCatResult<usize> {
    let requirements = parse_table_requirements(requirements)?;
    for requirement in &requirements {
        validate_stable_requirement(metadata, requirement)?;
    }
    Ok(requirements.len())
}

fn parse_table_requirements(requirements: &[Value]) -> LakeCatResult<Vec<TableRequirement>> {
    serde_json::from_value(Value::Array(requirements.to_vec())).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "invalid Iceberg REST table requirement for Sail: {err}"
        ))
    })
}

fn validate_stable_requirement(
    metadata: &TableMetadata,
    requirement: &TableRequirement,
) -> LakeCatResult<()> {
    match requirement {
        TableRequirement::NotExist => Err(LakeCatError::Conflict(
            "Iceberg table already exists but commit asserted non-existence".to_string(),
        )),
        TableRequirement::UuidMatch { uuid } => {
            if metadata.table_uuid.as_ref() == Some(uuid) {
                Ok(())
            } else {
                Err(LakeCatError::Conflict(format!(
                    "Iceberg commit failed: expected table UUID {uuid} but found {:?}",
                    metadata.table_uuid
                )))
            }
        }
        TableRequirement::RefSnapshotIdMatch { r#ref, snapshot_id } => {
            let actual = if r#ref == MAIN_BRANCH {
                metadata.current_snapshot_id
            } else {
                metadata
                    .refs
                    .get(r#ref)
                    .map(|ref_entry| ref_entry.snapshot_id)
            };
            let actual = actual.filter(|snapshot_id| *snapshot_id >= 0);
            if actual == *snapshot_id {
                Ok(())
            } else {
                Err(LakeCatError::Conflict(format!(
                    "Iceberg commit failed: reference '{ref}' expected snapshot {snapshot_id:?} but found {actual:?}"
                )))
            }
        }
        TableRequirement::LastAssignedFieldIdMatch {
            last_assigned_field_id,
        } => {
            if metadata.last_column_id == *last_assigned_field_id {
                Ok(())
            } else {
                Err(LakeCatError::Conflict(format!(
                    "Iceberg commit failed: expected last assigned field id {last_assigned_field_id} but found {}",
                    metadata.last_column_id
                )))
            }
        }
        TableRequirement::CurrentSchemaIdMatch { current_schema_id } => {
            if metadata.current_schema_id == *current_schema_id {
                Ok(())
            } else {
                Err(LakeCatError::Conflict(format!(
                    "Iceberg commit failed: expected current schema id {current_schema_id} but found {}",
                    metadata.current_schema_id
                )))
            }
        }
        TableRequirement::LastAssignedPartitionIdMatch {
            last_assigned_partition_id,
        } => {
            if metadata.last_partition_id == *last_assigned_partition_id {
                Ok(())
            } else {
                Err(LakeCatError::Conflict(format!(
                    "Iceberg commit failed: expected last assigned partition id {last_assigned_partition_id} but found {}",
                    metadata.last_partition_id
                )))
            }
        }
        TableRequirement::DefaultSpecIdMatch { default_spec_id } => {
            if metadata.default_spec_id == *default_spec_id {
                Ok(())
            } else {
                Err(LakeCatError::Conflict(format!(
                    "Iceberg commit failed: expected default partition spec id {default_spec_id} but found {}",
                    metadata.default_spec_id
                )))
            }
        }
        TableRequirement::DefaultSortOrderIdMatch {
            default_sort_order_id,
        } => {
            let actual = metadata.default_sort_order_id.map(i64::from).unwrap_or(0);
            if actual == *default_sort_order_id {
                Ok(())
            } else {
                Err(LakeCatError::Conflict(format!(
                    "Iceberg commit failed: expected default sort order id {default_sort_order_id} but found {actual}"
                )))
            }
        }
    }
}

fn validate_v4_extension_commit_requirements(
    metadata_summary: &SailMetadataSummary,
    requirements: &[Value],
) -> LakeCatResult<usize> {
    let mut validated = 0;
    for requirement in requirements {
        let Some(requirement_type) = requirement.get("type").and_then(Value::as_str) else {
            return Err(LakeCatError::InvalidArgument(
                "Iceberg table requirement is missing a type".to_string(),
            ));
        };
        match requirement_type {
            "assert-create" => {
                return Err(LakeCatError::Conflict(
                    "Iceberg table already exists but commit asserted non-existence".to_string(),
                ));
            }
            "assert-table-uuid" => {
                validate_json_requirement(
                    "table UUID",
                    metadata_summary.table_uuid.as_deref(),
                    requirement.get("uuid").and_then(Value::as_str),
                )?;
                validated += 1;
            }
            "assert-current-schema-id" => {
                validate_json_i64_requirement(
                    "current schema id",
                    metadata_summary.current_schema_id.map(i64::from),
                    requirement.get("current-schema-id").and_then(Value::as_i64),
                )?;
                validated += 1;
            }
            "assert-ref-snapshot-id" => {
                let reference = requirement
                    .get("ref")
                    .and_then(Value::as_str)
                    .unwrap_or(MAIN_BRANCH);
                if reference == MAIN_BRANCH {
                    validate_json_i64_requirement(
                        "main snapshot id",
                        metadata_summary.current_snapshot_id,
                        requirement.get("snapshot-id").and_then(Value::as_i64),
                    )?;
                }
                validated += 1;
            }
            "assert-last-assigned-field-id" => {
                validate_json_i64_requirement(
                    "last assigned field id",
                    metadata_summary.last_assigned_field_id.map(i64::from),
                    requirement
                        .get("last-assigned-field-id")
                        .and_then(Value::as_i64),
                )?;
                validated += 1;
            }
            "assert-last-assigned-partition-id" => {
                validate_json_i64_requirement(
                    "last assigned partition id",
                    metadata_summary.last_assigned_partition_id.map(i64::from),
                    requirement
                        .get("last-assigned-partition-id")
                        .and_then(Value::as_i64),
                )?;
                validated += 1;
            }
            "assert-default-spec-id" => {
                validate_json_i64_requirement(
                    "default partition spec id",
                    metadata_summary.default_spec_id.map(i64::from),
                    requirement.get("default-spec-id").and_then(Value::as_i64),
                )?;
                validated += 1;
            }
            "assert-default-sort-order-id" => {
                validate_json_i64_requirement(
                    "default sort order id",
                    metadata_summary.default_sort_order_id,
                    requirement
                        .get("default-sort-order-id")
                        .and_then(Value::as_i64),
                )?;
                validated += 1;
            }
            _ => {}
        }
    }
    Ok(validated)
}

fn validate_json_requirement(
    label: &str,
    actual: Option<&str>,
    expected: Option<&str>,
) -> LakeCatResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(LakeCatError::Conflict(format!(
            "Iceberg commit failed: expected {label} {expected:?} but found {actual:?}"
        )))
    }
}

fn validate_json_i64_requirement(
    label: &str,
    actual: Option<i64>,
    expected: Option<i64>,
) -> LakeCatResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(LakeCatError::Conflict(format!(
            "Iceberg commit failed: expected {label} {expected:?} but found {actual:?}"
        )))
    }
}

async fn stable_manifest_plan_tasks(
    metadata: &TableMetadata,
    metadata_summary: &SailMetadataSummary,
    request: &ScanPlanningRequest,
    metadata_location: Option<String>,
) -> LakeCatResult<Vec<Value>> {
    if request.is_incremental_scan() {
        return incremental_manifest_plan_tasks(
            metadata,
            metadata_summary,
            request,
            metadata_location,
        )
        .await;
    }
    let snapshot = selected_snapshot(metadata, request.snapshot_id)?;
    full_snapshot_manifest_plan_tasks(snapshot, request, metadata_location)
}

fn full_snapshot_manifest_plan_tasks(
    snapshot: &Snapshot,
    request: &ScanPlanningRequest,
    metadata_location: Option<String>,
) -> LakeCatResult<Vec<Value>> {
    let mut tasks = Vec::new();
    push_manifest_list_scan_task(&mut tasks, snapshot, request, metadata_location.clone())?;
    push_v1_manifest_scan_tasks(&mut tasks, snapshot, request, metadata_location)?;
    Ok(tasks)
}

async fn incremental_manifest_plan_tasks(
    metadata: &TableMetadata,
    metadata_summary: &SailMetadataSummary,
    request: &ScanPlanningRequest,
    metadata_location: Option<String>,
) -> LakeCatResult<Vec<Value>> {
    let snapshots =
        incremental_snapshot_chain(metadata, request.start_snapshot_id, request.end_snapshot_id)?;
    let Some(store_ctx) = local_store_context(metadata_summary)? else {
        return Err(LakeCatError::NotSupported(
                "Iceberg incremental scan planning currently requires local file metadata so Sail can inspect manifest lists"
                    .to_string(),
            ));
    };
    let mut tasks = Vec::new();
    for snapshot in snapshots {
        if snapshot.summary().operation.as_str() != "append" {
            return Err(LakeCatError::NotSupported(format!(
                "Iceberg incremental scan planning only supports append snapshots, but snapshot {} is {}",
                snapshot.snapshot_id(),
                snapshot.summary().operation.as_str()
            )));
        }
        if !snapshot.manifest_list().is_empty() {
            let manifest_list = load_manifest_list(&store_ctx, snapshot.manifest_list())
                .await
                .map_err(|err| {
                    LakeCatError::Internal(format!(
                        "failed to load Iceberg manifest list for incremental planning: {err}"
                    ))
                })?;
            let has_added_manifests = manifest_list
                .entries()
                .iter()
                .any(|manifest| manifest.added_snapshot_id == snapshot.snapshot_id());
            if has_added_manifests {
                tasks.push(scan_task_value(SailScanTask {
                    task_type: "incremental-manifest-list".to_string(),
                    table: request.table.stable_id(),
                    snapshot_id: snapshot.snapshot_id(),
                    plan_task: opaque_plan_task_with_filters(
                        &request.table,
                        "incremental-manifest-list",
                        snapshot.snapshot_id(),
                        snapshot.manifest_list(),
                        &request.projection,
                        &request.filters,
                    )?,
                    metadata_location: metadata_location.clone(),
                    manifest_list: Some(snapshot.manifest_list().to_string()),
                    manifest_path: None,
                    content: None,
                    sequence_number: Some(snapshot.sequence_number()),
                })?);
            }
        }
        push_v1_manifest_scan_tasks(&mut tasks, snapshot, request, metadata_location.clone())?;
    }
    Ok(tasks)
}

fn push_manifest_list_scan_task(
    tasks: &mut Vec<Value>,
    snapshot: &Snapshot,
    request: &ScanPlanningRequest,
    metadata_location: Option<String>,
) -> LakeCatResult<()> {
    if !snapshot.manifest_list().is_empty() {
        tasks.push(scan_task_value(SailScanTask {
            task_type: "manifest-list".to_string(),
            table: request.table.stable_id(),
            snapshot_id: snapshot.snapshot_id(),
            plan_task: opaque_plan_task_with_filters(
                &request.table,
                "manifest-list",
                snapshot.snapshot_id(),
                snapshot.manifest_list(),
                &request.projection,
                &request.filters,
            )?,
            metadata_location,
            manifest_list: Some(snapshot.manifest_list().to_string()),
            manifest_path: None,
            content: None,
            sequence_number: Some(snapshot.sequence_number()),
        })?);
    }
    Ok(())
}

fn push_v1_manifest_scan_tasks(
    tasks: &mut Vec<Value>,
    snapshot: &Snapshot,
    request: &ScanPlanningRequest,
    metadata_location: Option<String>,
) -> LakeCatResult<()> {
    if let Some(manifests) = snapshot.manifests() {
        for manifest_path in manifests {
            tasks.push(scan_task_value(SailScanTask {
                task_type: "manifest".to_string(),
                table: request.table.stable_id(),
                snapshot_id: snapshot.snapshot_id(),
                plan_task: opaque_plan_task_with_filters(
                    &request.table,
                    "manifest",
                    snapshot.snapshot_id(),
                    manifest_path,
                    &request.projection,
                    &request.filters,
                )?,
                metadata_location: metadata_location.clone(),
                manifest_list: None,
                manifest_path: Some(manifest_path.clone()),
                content: Some("data".to_string()),
                sequence_number: Some(snapshot.sequence_number()),
            })?);
        }
    }
    Ok(())
}

fn selected_snapshot(
    metadata: &TableMetadata,
    requested_snapshot_id: Option<i64>,
) -> LakeCatResult<&Snapshot> {
    if let Some(snapshot_id) = requested_snapshot_id {
        metadata
            .snapshots
            .iter()
            .find(|snapshot| snapshot.snapshot_id() == snapshot_id)
            .ok_or_else(|| {
                LakeCatError::InvalidArgument(format!("Iceberg snapshot {snapshot_id} not found"))
            })
    } else {
        metadata.current_snapshot().ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "Iceberg table metadata is missing a current snapshot".to_string(),
            )
        })
    }
}

fn incremental_snapshot_chain(
    metadata: &TableMetadata,
    start_snapshot_id: Option<i64>,
    end_snapshot_id: Option<i64>,
) -> LakeCatResult<Vec<&Snapshot>> {
    let start_snapshot_id = start_snapshot_id.ok_or_else(|| {
        LakeCatError::InvalidArgument(
            "Iceberg incremental scan planning requires start-snapshot-id".to_string(),
        )
    })?;
    let end_snapshot_id = end_snapshot_id.ok_or_else(|| {
        LakeCatError::InvalidArgument(
            "Iceberg incremental scan planning requires end-snapshot-id".to_string(),
        )
    })?;
    if start_snapshot_id == end_snapshot_id {
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();
    let mut cursor = selected_snapshot(metadata, Some(end_snapshot_id))?;
    loop {
        if cursor.snapshot_id() == start_snapshot_id {
            snapshots.reverse();
            return Ok(snapshots);
        }
        snapshots.push(cursor);
        let Some(parent_snapshot_id) = cursor.parent_snapshot_id() else {
            return Err(LakeCatError::InvalidArgument(format!(
                "Iceberg snapshot {start_snapshot_id} is not an ancestor of snapshot {end_snapshot_id}"
            )));
        };
        cursor = selected_snapshot(metadata, Some(parent_snapshot_id))?;
    }
}

fn v4_extension_manifest_plan_tasks(
    request: &ScanPlanningRequest,
    metadata_summary: &SailMetadataSummary,
    metadata_location: Option<String>,
) -> LakeCatResult<Vec<Value>> {
    let snapshot_id = request
        .snapshot_id
        .or(metadata_summary.current_snapshot_id)
        .ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "Iceberg v4 extension metadata is missing a current snapshot".to_string(),
            )
        })?;
    let mut tasks = Vec::new();
    if let Some(manifest_list) = &metadata_summary.manifest_list {
        tasks.push(scan_task_value(SailScanTask {
            task_type: "manifest-list".to_string(),
            table: request.table.stable_id(),
            snapshot_id,
            plan_task: opaque_plan_task_with_filters(
                &request.table,
                "manifest-list",
                snapshot_id,
                manifest_list,
                &request.projection,
                &request.filters,
            )?,
            metadata_location,
            manifest_list: Some(manifest_list.clone()),
            manifest_path: None,
            content: None,
            sequence_number: metadata_summary.sequence_number,
        })?);
    }
    Ok(tasks)
}

fn opaque_plan_task_with_filters(
    table: &TableIdent,
    kind: &str,
    snapshot_id: i64,
    path: &str,
    projection: &[String],
    filters: &[Value],
) -> LakeCatResult<String> {
    let payload = EncodedPlanTask {
        table: table.stable_id(),
        kind: kind.to_string(),
        snapshot_id,
        path: path.to_string(),
        projection: projection.to_vec(),
        filters: filters.to_vec(),
    };
    let bytes = serde_json::to_vec(&payload).map_err(|err| {
        LakeCatError::Internal(format!("failed to encode LakeCat/Sail plan task: {err}"))
    })?;
    let signature = sign_plan_task_payload(&bytes)?;
    Ok(format!(
        "lakecat:sail-json-hmac:{}:{}",
        signature,
        hex::encode(bytes)
    ))
}

fn scan_task_value(task: SailScanTask) -> LakeCatResult<Value> {
    serde_json::to_value(task)
        .map_err(|err| LakeCatError::Internal(format!("failed to encode Sail scan task: {err}")))
}

#[derive(Debug, Default)]
struct FetchExpansion {
    file_scan_tasks: Vec<Value>,
    delete_files: Vec<Value>,
    plan_tasks: Vec<Value>,
}

#[derive(Debug, Clone, Copy)]
enum ManifestListExpansionMode {
    FullSnapshot,
    AddedBySnapshot,
}

impl ManifestListExpansionMode {
    fn includes(
        &self,
        decoded: &DecodedPlanTask,
        manifest: &sail_iceberg::spec::ManifestFile,
    ) -> bool {
        self.includes_snapshot(manifest, decoded.snapshot_id)
    }

    fn includes_snapshot(
        &self,
        manifest: &sail_iceberg::spec::ManifestFile,
        snapshot_id: i64,
    ) -> bool {
        match self {
            Self::FullSnapshot => true,
            Self::AddedBySnapshot => manifest.added_snapshot_id == snapshot_id,
        }
    }
}

async fn expand_fetch_plan_task_with_sail_io(
    decoded: &DecodedPlanTask,
    metadata_summary: &SailMetadataSummary,
    typed_metadata: Option<&TableMetadata>,
    request: &FetchScanTasksRequest,
) -> LakeCatResult<Option<FetchExpansion>> {
    if !local_file_url_exists(&decoded.path) {
        return Ok(None);
    }
    let Some(store_ctx) = local_store_context(metadata_summary)? else {
        return Ok(None);
    };
    match decoded.kind.as_str() {
        "manifest-list" | "incremental-manifest-list" => {
            let mode = match decoded.kind.as_str() {
                "incremental-manifest-list" => ManifestListExpansionMode::AddedBySnapshot,
                _ => ManifestListExpansionMode::FullSnapshot,
            };
            expand_manifest_list_task(&store_ctx, decoded, request, typed_metadata, mode)
                .await
                .map(Some)
        }
        "manifest" => expand_manifest_task(&store_ctx, decoded).await.map(Some),
        _ => Ok(None),
    }
}

fn local_store_context(
    metadata_summary: &SailMetadataSummary,
) -> LakeCatResult<Option<StoreContext>> {
    let Some(table_location) = metadata_summary.table_location.as_deref() else {
        return Ok(None);
    };
    let table_url = match Url::parse(table_location) {
        Ok(url) if url.scheme() == "file" => url,
        _ => return Ok(None),
    };
    StoreContext::new(Arc::new(LocalFileSystem::new()), &table_url)
        .map(Some)
        .map_err(|err| {
            LakeCatError::Internal(format!("failed to create Sail store context: {err}"))
        })
}

async fn expand_manifest_list_task(
    store_ctx: &StoreContext,
    decoded: &DecodedPlanTask,
    request: &FetchScanTasksRequest,
    typed_metadata: Option<&TableMetadata>,
    mode: ManifestListExpansionMode,
) -> LakeCatResult<FetchExpansion> {
    let manifest_list = load_manifest_list(store_ctx, &decoded.path)
        .await
        .map_err(|err| {
            LakeCatError::Internal(format!("failed to load Iceberg manifest list: {err}"))
        })?;
    let plan_tasks = manifest_list
        .entries()
        .iter()
        .filter(|manifest| mode.includes(decoded, manifest))
        .map(|manifest| {
            scan_task_value(SailScanTask {
                task_type: "manifest".to_string(),
                table: request.table.stable_id(),
                snapshot_id: decoded.snapshot_id,
                plan_task: opaque_plan_task_with_filters(
                    &request.table,
                    "manifest",
                    decoded.snapshot_id,
                    &manifest.manifest_path,
                    &decoded.projection,
                    &decoded.filters,
                )?,
                metadata_location: request.metadata_location.clone(),
                manifest_list: Some(decoded.path.clone()),
                manifest_path: Some(manifest.manifest_path.clone()),
                content: Some(
                    match &manifest.content {
                        ManifestContentType::Data => "data",
                        ManifestContentType::Deletes => "deletes",
                    }
                    .to_string(),
                ),
                sequence_number: Some(manifest.sequence_number),
            })
        })
        .collect::<LakeCatResult<Vec<_>>>()?;
    let mut expansion = FetchExpansion {
        plan_tasks,
        ..FetchExpansion::default()
    };
    if let Some(metadata) = typed_metadata {
        expand_manifest_list_files_with_delete_refs(
            store_ctx,
            &manifest_list,
            metadata,
            &mut expansion,
            mode,
            decoded.snapshot_id,
            &decoded.filters,
        )
        .await?;
    }
    validate_fetch_tasks_shape(&expansion)?;
    Ok(expansion)
}

async fn expand_manifest_list_files_with_delete_refs(
    store_ctx: &StoreContext,
    manifest_list: &sail_iceberg::spec::ManifestList,
    metadata: &TableMetadata,
    expansion: &mut FetchExpansion,
    mode: ManifestListExpansionMode,
    snapshot_id: i64,
    filters: &[Value],
) -> LakeCatResult<()> {
    let mut data_files = Vec::new();
    let mut delete_index = DeleteFileIndex::new();
    for manifest_file in manifest_list.entries() {
        if !mode.includes_snapshot(manifest_file, snapshot_id) {
            continue;
        }
        let manifest = load_manifest(store_ctx, &manifest_file.manifest_path)
            .await
            .map_err(|err| {
                LakeCatError::Internal(format!(
                    "failed to load Iceberg manifest from manifest list: {err}"
                ))
            })?;
        let partition_spec_id = manifest_file.partition_spec_id;
        let parent_seq = manifest_file.sequence_number;
        let mut inherited_next_row_id = manifest_file.first_row_id;
        for entry in manifest.entries() {
            if !matches!(
                entry.status,
                ManifestStatus::Added | ManifestStatus::Existing
            ) {
                continue;
            }
            let mut file = entry.data_file.clone();
            file.partition_spec_id = partition_spec_id;
            let sequence_number = entry.sequence_number.unwrap_or(parent_seq);
            match manifest_file.content {
                ManifestContentType::Data => {
                    if file.first_row_id.is_none() {
                        file.first_row_id = inherited_next_row_id;
                    }
                    if let Some(next_row_id) = &mut inherited_next_row_id {
                        *next_row_id += checked_i64(file.record_count, "record count")?;
                    }
                    if file.content == DataContentType::Data {
                        data_files.push((file, sequence_number));
                    }
                }
                ManifestContentType::Deletes => {
                    let file_ref = DeleteFileRef {
                        data_file: file,
                        data_sequence_number: sequence_number,
                        partition_spec_id,
                        is_unpartitioned_spec: metadata
                            .partition_specs
                            .iter()
                            .find(|spec| spec.spec_id() == partition_spec_id)
                            .map(|spec| spec.is_unpartitioned())
                            .unwrap_or(false),
                    };
                    delete_index.insert(file_ref).map_err(|err| {
                        LakeCatError::NotSupported(format!(
                            "failed to index Iceberg delete file with Sail: {err}"
                        ))
                    })?;
                }
            }
        }
    }

    data_files = data_files
        .into_iter()
        .filter_map(|(data_file, sequence_number)| {
            match data_file_may_match_filters(metadata, &data_file, filters) {
                Ok(true) => Some(Ok((data_file, sequence_number))),
                Ok(false) => None,
                Err(err) => Some(Err(err)),
            }
        })
        .collect::<LakeCatResult<Vec<_>>>()?;

    let mut delete_key_to_index = std::collections::BTreeMap::new();
    for (data_file, sequence_number) in data_files {
        let matched = delete_index.for_data_file(&data_file, sequence_number);
        let mut delete_refs = Vec::new();
        for delete_file in matched.positional.iter().chain(matched.equality.iter()) {
            delete_refs.push(delete_file_reference_index(
                delete_file,
                &mut delete_key_to_index,
                &mut expansion.delete_files,
            )?);
        }
        let mut scan_task = json!({
            "data-file": rest_data_file_value(&data_file)?,
        });
        if !delete_refs.is_empty() {
            scan_task["delete-file-references"] = json!(delete_refs);
        }
        expansion.file_scan_tasks.push(scan_task);
    }
    Ok(())
}

fn delete_file_reference_index(
    delete_file: &DeleteFileRef,
    delete_key_to_index: &mut std::collections::BTreeMap<String, i32>,
    delete_files: &mut Vec<Value>,
) -> LakeCatResult<i32> {
    let key = delete_file_key(delete_file);
    if let Some(index) = delete_key_to_index.get(&key) {
        return Ok(*index);
    }
    let value = rest_delete_file_value(&delete_file.data_file)?;
    validate_delete_file_shape(&value)?;
    let index = i32::try_from(delete_files.len()).map_err(|_| {
        LakeCatError::NotSupported(
            "Iceberg delete file reference index exceeds i32 range".to_string(),
        )
    })?;
    delete_files.push(value);
    delete_key_to_index.insert(key, index);
    Ok(index)
}

fn delete_file_key(delete_file: &DeleteFileRef) -> String {
    format!(
        "{}:{}:{}",
        delete_file.data_sequence_number,
        delete_file.partition_spec_id,
        delete_file.data_file.file_path
    )
}

fn data_file_may_match_filters(
    metadata: &TableMetadata,
    data_file: &SailDataFile,
    filters: &[Value],
) -> LakeCatResult<bool> {
    for filter in filters {
        if !data_file_may_match_filter(metadata, data_file, filter)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn data_file_may_match_filter(
    metadata: &TableMetadata,
    data_file: &SailDataFile,
    filter: &Value,
) -> LakeCatResult<bool> {
    let expression_type = filter_type(filter)?;
    match expression_type {
        "true" | "always-true" => Ok(true),
        "false" | "always-false" => Ok(false),
        "and" => Ok(data_file_may_match_filter(
            metadata,
            data_file,
            required_filter_child(filter, "left")?,
        )? && data_file_may_match_filter(
            metadata,
            data_file,
            required_filter_child(filter, "right")?,
        )?),
        "or" => Ok(data_file_may_match_filter(
            metadata,
            data_file,
            required_filter_child(filter, "left")?,
        )? || data_file_may_match_filter(
            metadata,
            data_file,
            required_filter_child(filter, "right")?,
        )?),
        // Negation over file-level bounds is easy to make unsound. Keep the file.
        "not" => Ok(true),
        "in" => {
            let expression =
                validate_filter_model::<models::SetExpression>(filter, expression_type)?;
            let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                return Ok(true);
            };
            for value in &expression.values {
                if literal_predicate_may_match(metadata, data_file, field, "eq", value)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        "not-in" => {
            let expression =
                validate_filter_model::<models::SetExpression>(filter, expression_type)?;
            let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                return Ok(true);
            };
            if let Some(single_value) = exact_file_value(data_file, field.id) {
                let mut excluded = false;
                for value in &expression.values {
                    let literal = filter_literal_for_field(field, value)?;
                    if Some(&literal) == Some(single_value) {
                        excluded = true;
                        break;
                    }
                }
                Ok(!excluded)
            } else {
                Ok(true)
            }
        }
        "lt" | "lt-eq" | "gt" | "gt-eq" | "eq" | "not-eq" | "starts-with" | "not-starts-with" => {
            let expression =
                validate_filter_model::<models::LiteralExpression>(filter, expression_type)?;
            let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                return Ok(true);
            };
            literal_predicate_may_match(
                metadata,
                data_file,
                field,
                expression_type,
                &expression.value,
            )
        }
        "is-null" => {
            let expression =
                validate_filter_model::<models::UnaryExpression>(filter, expression_type)?;
            let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                return Ok(true);
            };
            Ok(data_file
                .null_value_counts
                .get(&field.id)
                .copied()
                .unwrap_or(0)
                > 0)
        }
        "not-null" => {
            let expression =
                validate_filter_model::<models::UnaryExpression>(filter, expression_type)?;
            let Some(field) = filter_reference_field(metadata, expression.term.as_ref()) else {
                return Ok(true);
            };
            let nulls = data_file
                .null_value_counts
                .get(&field.id)
                .copied()
                .unwrap_or(0);
            Ok(nulls < data_file.record_count)
        }
        "is-nan" | "not-nan" => Ok(true),
        _ => Ok(true),
    }
}

fn literal_predicate_may_match(
    _metadata: &TableMetadata,
    data_file: &SailDataFile,
    field: &sail_iceberg::spec::NestedFieldRef,
    op: &str,
    value: &Value,
) -> LakeCatResult<bool> {
    let literal = filter_literal_for_field(field, value)?;
    let lower = data_file
        .lower_bounds
        .get(&field.id)
        .map(|datum| &datum.literal);
    let upper = data_file
        .upper_bounds
        .get(&field.id)
        .map(|datum| &datum.literal);
    Ok(match op {
        "eq" => value_may_overlap_bounds(&literal, lower, upper),
        "not-eq" => exact_file_value(data_file, field.id) != Some(&literal),
        "lt" => lower.is_none_or(|lower| lower < &literal),
        "lt-eq" => lower.is_none_or(|lower| lower <= &literal),
        "gt" => upper.is_none_or(|upper| upper > &literal),
        "gt-eq" => upper.is_none_or(|upper| upper >= &literal),
        "starts-with" => string_prefix_may_overlap_bounds(&literal, lower, upper),
        "not-starts-with" => exact_file_value(data_file, field.id)
            .is_none_or(|value| !primitive_starts_with(value, &literal)),
        _ => true,
    })
}

fn value_may_overlap_bounds(
    value: &PrimitiveLiteral,
    lower: Option<&PrimitiveLiteral>,
    upper: Option<&PrimitiveLiteral>,
) -> bool {
    lower.is_none_or(|lower| lower <= value) && upper.is_none_or(|upper| value <= upper)
}

fn exact_file_value(data_file: &SailDataFile, field_id: i32) -> Option<&PrimitiveLiteral> {
    let lower = data_file
        .lower_bounds
        .get(&field_id)
        .map(|datum| &datum.literal)?;
    let upper = data_file
        .upper_bounds
        .get(&field_id)
        .map(|datum| &datum.literal)?;
    (lower == upper).then_some(lower)
}

fn string_prefix_may_overlap_bounds(
    prefix: &PrimitiveLiteral,
    lower: Option<&PrimitiveLiteral>,
    upper: Option<&PrimitiveLiteral>,
) -> bool {
    let PrimitiveLiteral::String(prefix) = prefix else {
        return true;
    };
    let lower = match lower {
        Some(PrimitiveLiteral::String(value)) => Some(value),
        Some(_) => return true,
        None => None,
    };
    let upper = match upper {
        Some(PrimitiveLiteral::String(value)) => Some(value),
        Some(_) => return true,
        None => None,
    };
    if upper.is_some_and(|upper| upper.as_str() < prefix.as_str()) {
        return false;
    }
    if let Some(next_prefix) = next_lexicographic_prefix(prefix) {
        if lower.is_some_and(|lower| lower.as_str() >= next_prefix.as_str()) {
            return false;
        }
    }
    true
}

fn primitive_starts_with(value: &PrimitiveLiteral, prefix: &PrimitiveLiteral) -> bool {
    match (value, prefix) {
        (PrimitiveLiteral::String(value), PrimitiveLiteral::String(prefix)) => {
            value.starts_with(prefix)
        }
        _ => false,
    }
}

fn next_lexicographic_prefix(prefix: &str) -> Option<String> {
    let mut bytes = prefix.as_bytes().to_vec();
    for index in (0..bytes.len()).rev() {
        if bytes[index] != u8::MAX {
            bytes[index] += 1;
            bytes.truncate(index + 1);
            return String::from_utf8(bytes).ok();
        }
    }
    None
}

fn filter_reference_field<'a>(
    metadata: &'a TableMetadata,
    term: &models::Term,
) -> Option<&'a sail_iceberg::spec::NestedFieldRef> {
    let reference = match term {
        models::Term::Reference(reference) => reference.as_str(),
        models::Term::TransformTerm(_) => return None,
    };
    metadata
        .current_schema()?
        .fields()
        .iter()
        .find(|field| field.name == reference)
}

fn filter_literal_for_field(
    field: &sail_iceberg::spec::NestedFieldRef,
    value: &Value,
) -> LakeCatResult<PrimitiveLiteral> {
    let literal = Literal::try_from_json(value.clone(), &field.field_type).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "invalid Iceberg REST filter literal for column {}: {err}",
            field.name
        ))
    })?;
    match literal {
        Some(Literal::Primitive(literal)) => Ok(literal),
        Some(_) => Err(LakeCatError::NotSupported(format!(
            "Iceberg REST filter literal for column {} is not primitive",
            field.name
        ))),
        None => Err(LakeCatError::NotSupported(format!(
            "Iceberg REST null literal pruning for column {} is not supported",
            field.name
        ))),
    }
}

async fn expand_manifest_task(
    store_ctx: &StoreContext,
    decoded: &DecodedPlanTask,
) -> LakeCatResult<FetchExpansion> {
    let manifest = load_manifest(store_ctx, &decoded.path)
        .await
        .map_err(|err| LakeCatError::Internal(format!("failed to load Iceberg manifest: {err}")))?;
    let mut expansion = FetchExpansion::default();
    for entry in manifest.entries() {
        let file = &entry.data_file;
        match file.content {
            DataContentType::Data => {
                expansion.file_scan_tasks.push(json!({
                    "data-file": rest_data_file_value(file)?,
                }));
            }
            DataContentType::PositionDeletes | DataContentType::EqualityDeletes => {
                expansion.delete_files.push(rest_delete_file_value(file)?);
            }
        }
    }
    validate_fetch_tasks_shape(&expansion)?;
    Ok(expansion)
}

fn validate_fetch_tasks_shape(expansion: &FetchExpansion) -> LakeCatResult<()> {
    for delete_file in &expansion.delete_files {
        validate_delete_file_shape(delete_file)?;
    }
    let rest_result = json!({
        "file-scan-tasks": (!expansion.file_scan_tasks.is_empty()).then_some(&expansion.file_scan_tasks),
        "plan-tasks": (!expansion.plan_tasks.is_empty()).then_some(Vec::<String>::new()),
    });
    let _: models::FetchScanTasksResult = serde_json::from_value(rest_result).map_err(|err| {
        LakeCatError::Internal(format!(
            "Sail manifest expansion produced an invalid Iceberg fetchScanTasks result: {err}"
        ))
    })?;
    Ok(())
}

fn validate_delete_file_shape(delete_file: &Value) -> LakeCatResult<()> {
    match delete_file.get("content").and_then(Value::as_str) {
        Some("position-deletes") => {
            let _: models::PositionDeleteFile =
                    serde_json::from_value(delete_file.clone()).map_err(|err| {
                        LakeCatError::Internal(format!(
                            "Sail manifest expansion produced an invalid Iceberg position delete file: {err}"
                        ))
                    })?;
            Ok(())
        }
        Some("equality-deletes") => {
            let _: models::EqualityDeleteFile =
                    serde_json::from_value(delete_file.clone()).map_err(|err| {
                        LakeCatError::Internal(format!(
                            "Sail manifest expansion produced an invalid Iceberg equality delete file: {err}"
                        ))
                    })?;
            Ok(())
        }
        Some(other) => Err(LakeCatError::Internal(format!(
            "invalid Iceberg delete file content: {other}"
        ))),
        None => Err(LakeCatError::Internal(
            "Iceberg delete file is missing content".to_string(),
        )),
    }
}

fn rest_data_file_value(file: &SailDataFile) -> LakeCatResult<Value> {
    let mut value = rest_base_file_value(file)?;
    value["content"] = json!("data");
    if let Some(first_row_id) = file.first_row_id {
        value["first-row-id"] = json!(first_row_id);
    }
    insert_count_map(&mut value, "column-sizes", &file.column_sizes)?;
    insert_count_map(&mut value, "value-counts", &file.value_counts)?;
    insert_count_map(&mut value, "null-value-counts", &file.null_value_counts)?;
    insert_count_map(&mut value, "nan-value-counts", &file.nan_value_counts)?;
    insert_value_map(&mut value, "lower-bounds", &file.lower_bounds)?;
    insert_value_map(&mut value, "upper-bounds", &file.upper_bounds)?;
    Ok(value)
}

fn rest_delete_file_value(file: &SailDataFile) -> LakeCatResult<Value> {
    match file.content {
        DataContentType::PositionDeletes => {
            let mut value = rest_base_file_value(file)?;
            value["content"] = json!("position-deletes");
            if let Some(content_offset) = file.content_offset {
                value["content-offset"] = json!(content_offset);
            }
            if let Some(content_size) = file.content_size_in_bytes {
                value["content-size-in-bytes"] = json!(content_size);
            }
            Ok(value)
        }
        DataContentType::EqualityDeletes => {
            let mut value = rest_base_file_value(file)?;
            value["content"] = json!("equality-deletes");
            if !file.equality_ids.is_empty() {
                value["equality-ids"] = json!(file.equality_ids);
            }
            Ok(value)
        }
        DataContentType::Data => Err(LakeCatError::Internal(
            "data file cannot be encoded as an Iceberg delete file".to_string(),
        )),
    }
}

fn rest_base_file_value(file: &SailDataFile) -> LakeCatResult<Value> {
    let mut value = json!({
        "file-path": file.file_path,
        "file-format": rest_file_format(file.file_format),
        "spec-id": file.partition_spec_id,
        "partition": rest_partition_values(&file.partition)?,
        "file-size-in-bytes": checked_i64(file.file_size_in_bytes, "file size")?,
        "record-count": checked_i64(file.record_count, "record count")?,
    });
    if !file.split_offsets.is_empty() {
        value["split-offsets"] = json!(file.split_offsets);
    }
    if let Some(sort_order_id) = file.sort_order_id {
        value["sort-order-id"] = json!(sort_order_id);
    }
    Ok(value)
}

fn rest_file_format(format: DataFileFormat) -> &'static str {
    match format {
        DataFileFormat::Avro => "avro",
        DataFileFormat::Orc => "orc",
        DataFileFormat::Parquet => "parquet",
        DataFileFormat::Puffin => "puffin",
    }
}

fn rest_partition_values(
    partition: &[Option<sail_iceberg::spec::Literal>],
) -> LakeCatResult<Vec<Value>> {
    partition
        .iter()
        .map(|literal| match literal {
            Some(literal) => rest_literal_value(literal),
            None => Ok(Value::Null),
        })
        .collect()
}

fn rest_literal_value(literal: &sail_iceberg::spec::Literal) -> LakeCatResult<Value> {
    match literal {
        sail_iceberg::spec::Literal::Primitive(value) => rest_primitive_literal_value(value),
        sail_iceberg::spec::Literal::Struct(fields) => {
            let mut object = serde_json::Map::with_capacity(fields.len());
            for (name, value) in fields {
                object.insert(
                    name.clone(),
                    match value {
                        Some(value) => rest_literal_value(value)?,
                        None => Value::Null,
                    },
                );
            }
            Ok(Value::Object(object))
        }
        sail_iceberg::spec::Literal::List(items) => items
            .iter()
            .map(|item| match item {
                Some(item) => rest_literal_value(item),
                None => Ok(Value::Null),
            })
            .collect::<LakeCatResult<Vec<_>>>()
            .map(Value::Array),
        sail_iceberg::spec::Literal::Map(entries) => entries
            .iter()
            .map(|(key, value)| {
                Ok(json!({
                    "key": rest_literal_value(key)?,
                    "value": match value {
                        Some(value) => rest_literal_value(value)?,
                        None => Value::Null,
                    },
                }))
            })
            .collect::<LakeCatResult<Vec<_>>>()
            .map(Value::Array),
    }
}

fn rest_primitive_literal_value(value: &PrimitiveLiteral) -> LakeCatResult<Value> {
    match value {
        PrimitiveLiteral::Boolean(value) => Ok(json!(value)),
        PrimitiveLiteral::Int(value) => Ok(json!(value)),
        PrimitiveLiteral::Long(value) => Ok(json!(value)),
        PrimitiveLiteral::Float(value) => Ok(json!(value.into_inner())),
        PrimitiveLiteral::Double(value) => Ok(json!(value.into_inner())),
        PrimitiveLiteral::Int128(value) => Ok(json!(value.to_string())),
        PrimitiveLiteral::String(value) => Ok(json!(value)),
        PrimitiveLiteral::UInt128(value) => Ok(json!(uuid::Uuid::from_u128(*value))),
        PrimitiveLiteral::Binary(value) => Ok(json!(hex::encode_upper(value))),
    }
}

fn insert_count_map(
    value: &mut Value,
    field: &str,
    map: &std::collections::HashMap<i32, u64>,
) -> LakeCatResult<()> {
    if map.is_empty() {
        return Ok(());
    }
    let mut entries = map.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| **key);
    value[field] = json!({
        "keys": entries.iter().map(|(key, _)| **key).collect::<Vec<_>>(),
        "values": entries
            .iter()
            .map(|(_, count)| checked_i64(**count, field))
            .collect::<LakeCatResult<Vec<_>>>()?,
    });
    Ok(())
}

fn insert_value_map(
    value: &mut Value,
    field: &str,
    map: &std::collections::HashMap<i32, Datum>,
) -> LakeCatResult<()> {
    if map.is_empty() {
        return Ok(());
    }
    let mut entries = map.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| **key);
    value[field] = json!({
        "keys": entries.iter().map(|(key, _)| **key).collect::<Vec<_>>(),
        "values": entries
            .iter()
            .map(|(_, datum)| rest_primitive_literal_value(&datum.literal))
            .collect::<LakeCatResult<Vec<_>>>()?,
    });
    Ok(())
}

fn checked_i64(value: u64, label: &str) -> LakeCatResult<i64> {
    i64::try_from(value).map_err(|_| {
        LakeCatError::NotSupported(format!(
            "Iceberg {label} value {value} exceeds the REST model i64 range"
        ))
    })
}

fn local_file_url_exists(raw: &str) -> bool {
    Url::parse(raw)
        .ok()
        .filter(|url| url.scheme() == "file")
        .and_then(|url| url.to_file_path().ok())
        .is_some_and(|path| path.exists())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedPlanTask {
    raw: String,
    table: Option<String>,
    kind: String,
    snapshot_id: i64,
    path: String,
    projection: Vec<String>,
    filters: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct EncodedPlanTask {
    table: String,
    kind: String,
    snapshot_id: i64,
    path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    projection: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    filters: Vec<Value>,
}

impl DecodedPlanTask {
    fn to_scan_task(
        &self,
        table: String,
        metadata_location: Option<String>,
        sequence_number: Option<i64>,
    ) -> SailScanTask {
        SailScanTask {
            task_type: self.kind.clone(),
            table,
            snapshot_id: self.snapshot_id,
            plan_task: self.raw.clone(),
            metadata_location,
            manifest_list: matches!(
                self.kind.as_str(),
                "manifest-list" | "incremental-manifest-list"
            )
            .then(|| self.path.clone()),
            manifest_path: (self.kind == "manifest").then(|| self.path.clone()),
            content: (self.kind == "manifest").then(|| "data".to_string()),
            sequence_number,
        }
    }
}

fn decode_plan_task(plan_task: &str) -> LakeCatResult<DecodedPlanTask> {
    if let Some(rest) = plan_task.strip_prefix("lakecat:sail-json-hmac:") {
        let mut parts = rest.splitn(2, ':');
        let Some(signature) = parts.next() else {
            return invalid_plan_task(plan_task);
        };
        let Some(encoded) = parts.next() else {
            return invalid_plan_task(plan_task);
        };
        let bytes = hex::decode(encoded).map_err(|_| {
            LakeCatError::InvalidArgument(format!(
                "invalid LakeCat/Sail signed plan task: {plan_task}"
            ))
        })?;
        verify_plan_task_signature(&bytes, signature)?;
        return decode_structured_plan_task(plan_task, &bytes);
    }
    if let Some(encoded) = plan_task.strip_prefix("lakecat:sail-json:") {
        let bytes = hex::decode(encoded).map_err(|_| {
            LakeCatError::InvalidArgument(format!(
                "invalid LakeCat/Sail structured plan task: {plan_task}"
            ))
        })?;
        return decode_structured_plan_task(plan_task, &bytes);
    }
    let mut parts = plan_task.splitn(5, ':');
    let Some(prefix) = parts.next() else {
        return invalid_plan_task(plan_task);
    };
    let Some(engine) = parts.next() else {
        return invalid_plan_task(plan_task);
    };
    let Some(kind) = parts.next() else {
        return invalid_plan_task(plan_task);
    };
    let Some(snapshot_id) = parts.next() else {
        return invalid_plan_task(plan_task);
    };
    let Some(path) = parts.next() else {
        return invalid_plan_task(plan_task);
    };
    if prefix != "lakecat" || engine != "sail" {
        return invalid_plan_task(plan_task);
    }
    if !matches!(
        kind,
        "manifest-list" | "incremental-manifest-list" | "manifest"
    ) {
        return invalid_plan_task(plan_task);
    }
    let snapshot_id = snapshot_id.parse::<i64>().map_err(|_| {
        LakeCatError::InvalidArgument(format!("invalid LakeCat/Sail plan task: {plan_task}"))
    })?;
    Ok(DecodedPlanTask {
        raw: plan_task.to_string(),
        table: None,
        kind: kind.to_string(),
        snapshot_id,
        path: path.to_string(),
        projection: Vec::new(),
        filters: Vec::new(),
    })
}

fn decode_structured_plan_task(plan_task: &str, bytes: &[u8]) -> LakeCatResult<DecodedPlanTask> {
    let task: EncodedPlanTask = serde_json::from_slice(bytes).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "invalid LakeCat/Sail structured plan task payload: {err}"
        ))
    })?;
    if !matches!(
        task.kind.as_str(),
        "manifest-list" | "incremental-manifest-list" | "manifest"
    ) {
        return invalid_plan_task(plan_task);
    }
    Ok(DecodedPlanTask {
        raw: plan_task.to_string(),
        table: Some(task.table),
        kind: task.kind,
        snapshot_id: task.snapshot_id,
        path: task.path,
        projection: task.projection,
        filters: task.filters,
    })
}

fn sign_plan_task_payload(bytes: &[u8]) -> LakeCatResult<String> {
    let mut mac = HmacSha256::new_from_slice(&plan_task_signing_key()).map_err(|err| {
        LakeCatError::Internal(format!("failed to initialize plan-task HMAC: {err}"))
    })?;
    mac.update(bytes);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn verify_plan_task_signature(bytes: &[u8], signature: &str) -> LakeCatResult<()> {
    let signature = hex::decode(signature).map_err(|_| {
        LakeCatError::InvalidArgument("invalid LakeCat/Sail plan task signature".to_string())
    })?;
    let mut mac = HmacSha256::new_from_slice(&plan_task_signing_key()).map_err(|err| {
        LakeCatError::Internal(format!("failed to initialize plan-task HMAC: {err}"))
    })?;
    mac.update(bytes);
    mac.verify_slice(&signature).map_err(|_| {
        LakeCatError::InvalidArgument("invalid LakeCat/Sail plan task signature".to_string())
    })
}

fn plan_task_signing_key() -> Vec<u8> {
    std::env::var(PLAN_TASK_SIGNING_KEY_ENV)
        .map(|value| value.into_bytes())
        .unwrap_or_else(|_| DEFAULT_PLAN_TASK_SIGNING_KEY.to_vec())
}

fn invalid_plan_task<T>(plan_task: &str) -> LakeCatResult<T> {
    Err(LakeCatError::InvalidArgument(format!(
        "invalid LakeCat/Sail plan task: {plan_task}"
    )))
}

async fn validate_decoded_plan_task(
    request: &FetchScanTasksRequest,
    metadata_summary: &SailMetadataSummary,
    typed_metadata: Option<&TableMetadata>,
    decoded: &DecodedPlanTask,
) -> LakeCatResult<()> {
    if let Some(table) = &decoded.table
        && table != &request.table.stable_id()
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "plan task table does not match requested table {}",
            request.table.stable_id()
        )));
    }
    validate_decoded_projection(request, decoded)?;
    validate_decoded_filters(request, decoded)?;
    if let Some(metadata) = typed_metadata {
        let snapshot = selected_snapshot(metadata, Some(decoded.snapshot_id))?;
        match decoded.kind.as_str() {
            "manifest-list" | "incremental-manifest-list" => {
                if snapshot.manifest_list() == decoded.path {
                    Ok(())
                } else {
                    Err(LakeCatError::InvalidArgument(format!(
                        "plan task manifest list does not match snapshot {}",
                        decoded.snapshot_id
                    )))
                }
            }
            "manifest" => {
                if snapshot
                    .manifests()
                    .map(|manifests| manifests.iter().any(|path| path == &decoded.path))
                    .unwrap_or(false)
                    || manifest_path_matches_local_manifest_list(
                        metadata_summary,
                        snapshot,
                        &decoded.path,
                    )
                    .await?
                {
                    Ok(())
                } else {
                    Err(LakeCatError::InvalidArgument(format!(
                        "plan task manifest does not match snapshot {}",
                        decoded.snapshot_id
                    )))
                }
            }
            _ => invalid_plan_task(&decoded.raw),
        }
    } else {
        let snapshot_matches = request.snapshot_id_matches(decoded.snapshot_id, metadata_summary);
        let manifest_matches = metadata_summary.manifest_list.as_ref() == Some(&decoded.path);
        if snapshot_matches && decoded.kind == "manifest-list" && manifest_matches {
            Ok(())
        } else {
            Err(LakeCatError::InvalidArgument(format!(
                "plan task does not match Iceberg v4 extension metadata: {}",
                decoded.raw
            )))
        }
    }
}

fn validate_decoded_projection(
    request: &FetchScanTasksRequest,
    decoded: &DecodedPlanTask,
) -> LakeCatResult<()> {
    if request.required_projection.is_empty() {
        return Ok(());
    }
    if decoded.projection.is_empty() {
        return Err(LakeCatError::InvalidArgument(
            "plan task omits the required governed projection".to_string(),
        ));
    }
    if decoded.projection.iter().all(|column| {
        request
            .required_projection
            .iter()
            .any(|required| required == column)
    }) {
        Ok(())
    } else {
        Err(LakeCatError::InvalidArgument(
            "plan task projection widens the governed read restriction".to_string(),
        ))
    }
}

fn validate_decoded_filters(
    request: &FetchScanTasksRequest,
    decoded: &DecodedPlanTask,
) -> LakeCatResult<()> {
    for required in &request.required_filters {
        if !decoded.filters.iter().any(|filter| filter == required) {
            return Err(LakeCatError::InvalidArgument(
                "plan task omits a required governed filter".to_string(),
            ));
        }
    }
    Ok(())
}

async fn manifest_path_matches_local_manifest_list(
    metadata_summary: &SailMetadataSummary,
    snapshot: &Snapshot,
    manifest_path: &str,
) -> LakeCatResult<bool> {
    let manifest_list_path = snapshot.manifest_list();
    if manifest_list_path.is_empty() || !local_file_url_exists(manifest_list_path) {
        return Ok(false);
    }
    let Some(store_ctx) = local_store_context(metadata_summary)? else {
        return Ok(false);
    };
    let manifest_list = load_manifest_list(&store_ctx, manifest_list_path)
        .await
        .map_err(|err| {
            LakeCatError::Internal(format!(
                "failed to validate manifest against Iceberg manifest list: {err}"
            ))
        })?;
    Ok(manifest_list
        .entries()
        .iter()
        .any(|manifest| manifest.manifest_path == manifest_path))
}

trait FetchScanTasksRequestExt {
    fn snapshot_id_matches(&self, snapshot_id: i64, metadata_summary: &SailMetadataSummary)
    -> bool;
}

impl FetchScanTasksRequestExt for FetchScanTasksRequest {
    fn snapshot_id_matches(
        &self,
        snapshot_id: i64,
        metadata_summary: &SailMetadataSummary,
    ) -> bool {
        metadata_summary.current_snapshot_id == Some(snapshot_id)
    }
}

fn validate_projection(
    metadata_summary: &SailMetadataSummary,
    projection: &[String],
) -> LakeCatResult<()> {
    if metadata_summary.v4_extension_mode || metadata_summary.fields.is_empty() {
        return Ok(());
    }
    let missing = projection
        .iter()
        .filter(|column| {
            !metadata_summary
                .fields
                .iter()
                .any(|field| field.name == **column)
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(LakeCatError::InvalidArgument(format!(
            "unknown Iceberg projection column(s): {}",
            missing
                .into_iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }
}

fn validate_scan_filters(
    metadata_summary: &SailMetadataSummary,
    filters: &[Value],
) -> LakeCatResult<Vec<SailFilterSummary>> {
    filters
        .iter()
        .map(|filter| validate_scan_filter(metadata_summary, filter))
        .collect()
}

fn validate_scan_filter(
    metadata_summary: &SailMetadataSummary,
    filter: &Value,
) -> LakeCatResult<SailFilterSummary> {
    let mut references = Vec::new();
    let expression_type = validate_filter_expression(metadata_summary, filter, &mut references)?;
    references.sort();
    references.dedup();
    Ok(SailFilterSummary {
        expression_type,
        references,
        filter: filter.clone(),
    })
}

fn validate_filter_expression(
    metadata_summary: &SailMetadataSummary,
    filter: &Value,
    references: &mut Vec<String>,
) -> LakeCatResult<String> {
    let expression_type = filter_type(filter)?;
    match expression_type {
        "true" | "always-true" => {
            validate_filter_model::<models::TrueExpression>(filter, expression_type)?;
        }
        "false" | "always-false" => {
            validate_filter_model::<models::FalseExpression>(filter, expression_type)?;
        }
        "and" | "or" => {
            validate_filter_model::<models::AndOrExpression>(filter, expression_type)?;
            validate_filter_expression(
                metadata_summary,
                required_filter_child(filter, "left")?,
                references,
            )?;
            validate_filter_expression(
                metadata_summary,
                required_filter_child(filter, "right")?,
                references,
            )?;
        }
        "not" => {
            validate_filter_model::<models::NotExpression>(filter, expression_type)?;
            validate_filter_expression(
                metadata_summary,
                required_filter_child(filter, "child")?,
                references,
            )?;
        }
        "in" | "not-in" => {
            let expression =
                validate_filter_model::<models::SetExpression>(filter, expression_type)?;
            validate_filter_term(metadata_summary, expression.term.as_ref(), references)?;
        }
        "lt" | "lt-eq" | "gt" | "gt-eq" | "eq" | "not-eq" | "starts-with" | "not-starts-with" => {
            let expression =
                validate_filter_model::<models::LiteralExpression>(filter, expression_type)?;
            validate_filter_term(metadata_summary, expression.term.as_ref(), references)?;
        }
        "is-null" | "not-null" | "is-nan" | "not-nan" => {
            let expression =
                validate_filter_model::<models::UnaryExpression>(filter, expression_type)?;
            validate_filter_term(metadata_summary, expression.term.as_ref(), references)?;
        }
        other => {
            return Err(LakeCatError::NotSupported(format!(
                "unsupported Iceberg REST filter expression type: {other}"
            )));
        }
    }
    Ok(expression_type.to_string())
}

fn validate_filter_model<T>(filter: &Value, expression_type: &str) -> LakeCatResult<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(filter.clone()).map_err(|err| {
        LakeCatError::InvalidArgument(format!(
            "invalid Iceberg REST {expression_type} filter expression: {err}"
        ))
    })
}

fn filter_type(filter: &Value) -> LakeCatResult<&str> {
    filter.get("type").and_then(Value::as_str).ok_or_else(|| {
        LakeCatError::InvalidArgument(
            "Iceberg REST filter expression is missing string field 'type'".to_string(),
        )
    })
}

fn required_filter_child<'a>(filter: &'a Value, field: &str) -> LakeCatResult<&'a Value> {
    filter.get(field).ok_or_else(|| {
        LakeCatError::InvalidArgument(format!(
            "Iceberg REST filter expression is missing field '{field}'"
        ))
    })
}

fn validate_filter_term(
    metadata_summary: &SailMetadataSummary,
    term: &models::Term,
    references: &mut Vec<String>,
) -> LakeCatResult<()> {
    match term {
        models::Term::Reference(reference) => {
            validate_filter_reference(metadata_summary, reference)?;
            references.push(reference.clone());
            Ok(())
        }
        models::Term::TransformTerm(transform) => {
            validate_filter_reference(metadata_summary, &transform.term)?;
            references.push(transform.term.clone());
            Ok(())
        }
    }
}

fn validate_filter_reference(
    metadata_summary: &SailMetadataSummary,
    reference: &str,
) -> LakeCatResult<()> {
    if metadata_summary.v4_extension_mode || metadata_summary.fields.is_empty() {
        return Ok(());
    }
    if metadata_summary
        .fields
        .iter()
        .any(|field| field.name == reference)
    {
        Ok(())
    } else {
        Err(LakeCatError::InvalidArgument(format!(
            "unknown Iceberg filter column: {reference}"
        )))
    }
}

fn combined_scan_filter(filters: &[Value]) -> Option<Value> {
    let mut filters = filters.iter().cloned();
    let first = filters.next()?;
    Some(filters.fold(first, |left, right| {
        json!({
            "type": "and",
            "left": left,
            "right": right,
        })
    }))
}

fn iceberg_json_fields(metadata: &Value) -> Vec<SailFieldSummary> {
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
        .map(|fields| {
            fields
                .iter()
                .filter_map(|field| {
                    Some(SailFieldSummary {
                        id: field.get("id").and_then(Value::as_i64)? as i32,
                        name: field.get("name").and_then(Value::as_str)?.to_string(),
                        data_type: match field.get("type") {
                            Some(Value::String(value)) => value.clone(),
                            Some(value) => value.to_string(),
                            None => "unknown".to_string(),
                        },
                        required: field
                            .get("required")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        doc: field
                            .get("doc")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
