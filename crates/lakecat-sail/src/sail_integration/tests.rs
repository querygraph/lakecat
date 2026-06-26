use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
use sail_iceberg::spec::{
    DataContentType, DataFile, DataFileFormat, FormatVersion, Literal, ManifestContentType,
    ManifestFile, ManifestListWriter, ManifestMetadata, ManifestWriterBuilder, PrimitiveLiteral,
    PrimitiveType,
};
use url::Url;

use super::*;

#[tokio::test]
async fn validates_scan_with_sail_rest_models() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let plan = engine
        .plan_scan(ScanPlanningRequest {
            table,
            principal: Principal::anonymous(),
            metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            table_metadata: sample_metadata(3),
            projection: vec!["id".to_string()],
            filters: Vec::new(),
            limit: Some(10),
            snapshot_id: Some(42),
            start_snapshot_id: None,
            end_snapshot_id: None,
        })
        .await
        .expect("scan request should validate");
    assert_eq!(plan.planned_by, "sail-rest-models");
    assert_eq!(plan.snapshot_id, Some(42));
    assert_eq!(plan.scan_tasks.len(), 1);
    assert_eq!(
        plan.scan_tasks[0].pointer("/task-type"),
        Some(&json!("manifest-list"))
    );
    assert_eq!(
        plan.scan_tasks[0].pointer("/manifest-list"),
        Some(&json!("file:///tmp/events/metadata/snap-42.avro"))
    );
    assert_eq!(
        plan.residual_filter
            .unwrap()
            .pointer("/sail-metadata/fields/0/name"),
        Some(&json!("id"))
    );
}

#[tokio::test]
async fn rejects_unknown_format_versions_but_allows_v4_extension_mode() {
    assert!(validate_sail_table_metadata(&json!({"format-version": 4})).is_ok());
    assert!(validate_sail_table_metadata(&json!({"format-version": 9})).is_err());
}

#[tokio::test]
async fn inspects_v4_extension_metadata_without_typed_sail_claims() {
    let summary = inspect_sail_table_metadata(&sample_metadata(4))
        .expect("v4 extension metadata should inspect through JSON bridge");
    assert_eq!(summary.format_version, 4);
    assert!(summary.v4_extension_mode);
    assert_eq!(
        summary.table_uuid.as_deref(),
        Some("11111111-1111-1111-1111-111111111111")
    );
    assert_eq!(summary.current_schema_id, Some(1));
    assert_eq!(summary.current_snapshot_id, Some(42));
    assert_eq!(summary.sequence_number, Some(7));
    assert_eq!(summary.default_spec_id, Some(0));
    assert_eq!(
        summary.manifest_list.as_deref(),
        Some("file:///tmp/events/metadata/snap-42.avro")
    );
    assert_eq!(
        summary
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id"]
    );
}

#[tokio::test]
async fn plans_v4_extension_manifest_list_without_pruning_claims() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let plan = engine
        .plan_scan(ScanPlanningRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            metadata_location: Some("file:///tmp/events/metadata/v4.json".to_string()),
            table_metadata: sample_metadata(4),
            projection: vec!["id".to_string()],
            filters: vec![json!({
                "type": "eq",
                "term": "id",
                "value": "evt-1"
            })],
            limit: None,
            snapshot_id: None,
            start_snapshot_id: None,
            end_snapshot_id: None,
        })
        .await
        .expect("v4 extension scan should produce manifest-list task");
    assert_eq!(plan.planned_by, "sail-rest-models");
    assert_eq!(plan.snapshot_id, Some(42));
    assert_eq!(plan.scan_tasks.len(), 1);
    assert_eq!(
        plan.scan_tasks[0].pointer("/task-type"),
        Some(&json!("manifest-list"))
    );
    assert_eq!(
        plan.scan_tasks[0].pointer("/manifest-list"),
        Some(&json!("file:///tmp/events/metadata/snap-42.avro"))
    );
    let residual = plan
        .residual_filter
        .expect("v4 planning should explain bridge");
    assert_eq!(
        residual.pointer("/sail-metadata/v4-extension-mode"),
        Some(&json!(true))
    );
    assert_eq!(
        residual.pointer("/sail-metadata/fields/0/name"),
        Some(&json!("id"))
    );
    assert_eq!(residual.pointer("/select"), Some(&json!(["id"])));
    assert_eq!(
        residual.pointer("/filters-accepted-by-sail/0/expression-type"),
        Some(&json!("eq"))
    );
    assert_eq!(
        residual.pointer("/filters-accepted-by-sail/0/references/0"),
        Some(&json!("id"))
    );
}

#[tokio::test]
async fn fetches_v4_extension_manifest_list_plan_task_without_typed_metadata() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let metadata_location = "file:///tmp/events/metadata/v4.json".to_string();
    let filter = json!({
        "type": "eq",
        "term": "id",
        "value": "evt-1"
    });
    let plan = engine
        .plan_scan(ScanPlanningRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location.clone()),
            table_metadata: sample_metadata(4),
            projection: vec!["id".to_string()],
            filters: vec![filter.clone()],
            limit: None,
            snapshot_id: None,
            start_snapshot_id: None,
            end_snapshot_id: None,
        })
        .await
        .expect("v4 extension scan should produce a signed plan task");
    let plan_task = plan.scan_tasks[0]["plan-task"]
        .as_str()
        .expect("v4 plan should carry a plan-task token")
        .to_string();

    let fetched = engine
        .fetch_scan_tasks(FetchScanTasksRequest {
            table,
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location),
            table_metadata: sample_metadata(4),
            plan_task,
            required_projection: vec!["id".to_string()],
            required_filters: vec![filter],
        })
        .await
        .expect("v4 extension fetch should validate the JSON bridge task");

    assert_eq!(fetched.planned_by, "sail-rest-models");
    assert_eq!(fetched.snapshot_id, Some(42));
    assert_eq!(fetched.file_scan_tasks.len(), 0);
    assert_eq!(fetched.delete_files.len(), 0);
    assert_eq!(fetched.plan_tasks.len(), 1);
    assert_eq!(
        fetched.plan_tasks[0].pointer("/task-type"),
        Some(&json!("manifest-list"))
    );
    let residual = fetched
        .residual_filter
        .expect("fetch should explain the v4 JSON bridge");
    assert_eq!(
        residual.pointer("/lakecat:sail-target"),
        Some(&json!("sail_iceberg::io::load_manifest_list"))
    );
    assert_eq!(
        residual.pointer("/task-kind"),
        Some(&json!("manifest-list"))
    );
    assert_eq!(
        residual.pointer("/manifest-path"),
        Some(&json!("file:///tmp/events/metadata/snap-42.avro"))
    );
    assert_eq!(
        residual.pointer("/sail-metadata/v4-extension-mode"),
        Some(&json!(true))
    );
    assert_eq!(residual.pointer("/projection"), Some(&json!(["id"])));
    assert_eq!(residual.pointer("/filters/0/type"), Some(&json!("eq")));
}

#[test]
fn encodes_null_and_nested_partition_literals_for_iceberg_rest() {
    let partition = rest_partition_values(&[
        None,
        Some(Literal::Struct(vec![
            (
                "region".to_string(),
                Some(Literal::Primitive(PrimitiveLiteral::String(
                    "west".to_string(),
                ))),
            ),
            ("bucket".to_string(), None),
        ])),
        Some(Literal::List(vec![
            Some(Literal::Primitive(PrimitiveLiteral::Int(7))),
            None,
        ])),
        Some(Literal::Map(vec![(
            Literal::Primitive(PrimitiveLiteral::String("tier".to_string())),
            Some(Literal::Primitive(PrimitiveLiteral::String(
                "gold".to_string(),
            ))),
        )])),
    ])
    .expect("partition literals should encode as REST JSON");

    assert_eq!(
        Value::Array(partition),
        json!([
            null,
            {
                "region": "west",
                "bucket": null
            },
            [7, null],
            [
                {
                    "key": "tier",
                    "value": "gold"
                }
            ]
        ])
    );
}

#[tokio::test]
async fn rejects_v4_extension_plan_task_for_drifted_manifest_list() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let plan = engine
        .plan_scan(ScanPlanningRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            metadata_location: Some("file:///tmp/events/metadata/v4.json".to_string()),
            table_metadata: sample_metadata(4),
            projection: vec!["id".to_string()],
            filters: Vec::new(),
            limit: None,
            snapshot_id: None,
            start_snapshot_id: None,
            end_snapshot_id: None,
        })
        .await
        .expect("v4 extension scan should produce a signed plan task");

    let err = engine
        .fetch_scan_tasks(FetchScanTasksRequest {
            table,
            principal: Principal::anonymous(),
            metadata_location: Some("file:///tmp/events/metadata/v4.json".to_string()),
            table_metadata: sample_metadata_with_locations(
                4,
                "file:///tmp/events".to_string(),
                "file:///tmp/events/metadata/snap-99.avro".to_string(),
            ),
            plan_task: plan.scan_tasks[0]["plan-task"]
                .as_str()
                .expect("v4 plan should carry a plan-task token")
                .to_string(),
            required_projection: vec!["id".to_string()],
            required_filters: Vec::new(),
        })
        .await
        .expect_err("v4 bridge fetch should reject a drifted manifest list");

    assert!(
        err.to_string()
            .contains("plan task does not match Iceberg v4 extension metadata")
    );
}

#[tokio::test]
async fn validates_projected_columns_against_sail_schema() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let err = engine
        .plan_scan(ScanPlanningRequest {
            table,
            principal: Principal::anonymous(),
            metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            table_metadata: sample_metadata(3),
            projection: vec!["missing".to_string()],
            filters: Vec::new(),
            limit: None,
            snapshot_id: None,
            start_snapshot_id: None,
            end_snapshot_id: None,
        })
        .await
        .expect_err("missing projected column should fail");
    assert!(err.to_string().contains("unknown Iceberg projection"));
}

#[tokio::test]
async fn validates_scan_filters_against_sail_rest_models_and_schema() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let plan = engine
        .plan_scan(ScanPlanningRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            table_metadata: sample_metadata(3),
            projection: vec!["id".to_string()],
            filters: vec![json!({
                "type": "eq",
                "term": "id",
                "value": "evt-1"
            })],
            limit: None,
            snapshot_id: Some(42),
            start_snapshot_id: None,
            end_snapshot_id: None,
        })
        .await
        .expect("filter should validate against Sail REST models");
    let residual = plan.residual_filter.unwrap();
    assert_eq!(
        residual.pointer("/filters-accepted-by-sail/0/expression-type"),
        Some(&json!("eq"))
    );
    assert_eq!(
        residual.pointer("/filters-accepted-by-sail/0/references/0"),
        Some(&json!("id"))
    );

    let err = engine
        .plan_scan(ScanPlanningRequest {
            table,
            principal: Principal::anonymous(),
            metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            table_metadata: sample_metadata(3),
            projection: Vec::new(),
            filters: vec![json!({
                "type": "eq",
                "term": "missing",
                "value": "evt-1"
            })],
            limit: None,
            snapshot_id: Some(42),
            start_snapshot_id: None,
            end_snapshot_id: None,
        })
        .await
        .expect_err("unknown filter column should fail");
    assert!(err.to_string().contains("unknown Iceberg filter column"));
}

#[tokio::test]
async fn validates_commit_requirements_against_sail_metadata() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let plan = engine
        .prepare_commit(CommitPreparationRequest {
            table,
            principal: Principal::anonymous(),
            current_metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            new_metadata_location: None,
            current_metadata: sample_metadata(3),
            new_metadata: None,
            requirements: vec![
                json!({
                    "type": "assert-table-uuid",
                    "uuid": "11111111-1111-1111-1111-111111111111"
                }),
                json!({
                    "type": "assert-current-schema-id",
                    "current-schema-id": 1
                }),
                json!({
                    "type": "assert-ref-snapshot-id",
                    "ref": "main",
                    "snapshot-id": 42
                }),
            ],
            updates: Vec::new(),
        })
        .await
        .expect("requirements should match current Sail metadata");
    assert_eq!(
        plan.metadata_patch["lakecat:validated-requirements"],
        json!(3)
    );
    assert_eq!(
        plan.new_metadata_location.as_deref(),
        Some("file:///tmp/events/metadata/00000.json")
    );
}

#[tokio::test]
async fn commit_plan_accepts_lakecat_metadata_location_extension() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let plan = engine
        .prepare_commit(CommitPreparationRequest {
            table,
            principal: Principal::anonymous(),
            current_metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            new_metadata_location: Some("file:///tmp/events/metadata/00001.json".to_string()),
            current_metadata: sample_metadata(3),
            new_metadata: Some(sample_metadata(3)),
            requirements: Vec::new(),
            updates: Vec::new(),
        })
        .await
        .expect("metadata location extension should plan");

    assert_eq!(
        plan.new_metadata_location.as_deref(),
        Some("file:///tmp/events/metadata/00001.json")
    );
    assert_eq!(
        plan.metadata_patch["new-metadata-location"],
        json!("file:///tmp/events/metadata/00001.json")
    );
    assert!(plan.metadata_write_required);
}

#[tokio::test]
async fn rejects_stale_commit_requirements() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let err = engine
        .prepare_commit(CommitPreparationRequest {
            table,
            principal: Principal::anonymous(),
            current_metadata_location: Some("file:///tmp/events/metadata/00000.json".to_string()),
            new_metadata_location: None,
            current_metadata: sample_metadata(3),
            new_metadata: None,
            requirements: vec![json!({
                "type": "assert-current-schema-id",
                "current-schema-id": 9
            })],
            updates: Vec::new(),
        })
        .await
        .expect_err("stale schema requirement should fail");
    assert!(err.to_string().contains("expected current schema id 9"));
}

#[tokio::test]
async fn validates_v4_extension_commit_requirements_against_json_summary() {
    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let plan = engine
        .prepare_commit(CommitPreparationRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            current_metadata_location: Some("file:///tmp/events/metadata/v4.json".to_string()),
            new_metadata_location: None,
            current_metadata: sample_metadata(4),
            new_metadata: None,
            requirements: vec![
                json!({
                    "type": "assert-table-uuid",
                    "uuid": "11111111-1111-1111-1111-111111111111"
                }),
                json!({
                    "type": "assert-current-schema-id",
                    "current-schema-id": 1
                }),
                json!({
                    "type": "assert-ref-snapshot-id",
                    "ref": "main",
                    "snapshot-id": 42
                }),
                json!({
                    "type": "assert-last-assigned-field-id",
                    "last-assigned-field-id": 1
                }),
                json!({
                    "type": "assert-default-spec-id",
                    "default-spec-id": 0
                }),
            ],
            updates: Vec::new(),
        })
        .await
        .expect("v4 JSON bridge should validate stable commit requirements");
    assert_eq!(
        plan.metadata_patch["lakecat:validated-requirements"],
        json!(5)
    );

    let err = engine
        .prepare_commit(CommitPreparationRequest {
            table,
            principal: Principal::anonymous(),
            current_metadata_location: Some("file:///tmp/events/metadata/v4.json".to_string()),
            new_metadata_location: None,
            current_metadata: sample_metadata(4),
            new_metadata: None,
            requirements: vec![json!({
                "type": "assert-ref-snapshot-id",
                "ref": "main",
                "snapshot-id": 99
            })],
            updates: Vec::new(),
        })
        .await
        .expect_err("stale v4 snapshot requirement should fail");
    assert!(err.to_string().contains("expected main snapshot id"));
    assert!(err.to_string().contains("99"));
}

#[tokio::test]
async fn expands_local_manifest_list_with_sail_io() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-manifest-list-{unique}"));
    let table_dir = root.join("table");
    let metadata_dir = table_dir.join("metadata");
    fs::create_dir_all(&metadata_dir).unwrap();

    let manifest_list_path = metadata_dir.join("snap-42.avro");
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
    let table_location = Url::from_directory_path(&table_dir).unwrap().to_string();
    let manifest_list = Url::from_file_path(&manifest_list_path)
        .unwrap()
        .to_string();
    let metadata_location = format!("{table_location}metadata/00000.json");
    let metadata = sample_metadata_with_locations(3, table_location.clone(), manifest_list.clone());
    let table_metadata = TableMetadata::from_json(&serde_json::to_vec(&metadata).unwrap()).unwrap();
    let manifest_metadata = ManifestMetadata::new(
        Arc::new(table_metadata.current_schema().unwrap().clone()),
        table_metadata.current_schema_id,
        table_metadata.default_partition_spec().unwrap().clone(),
        FormatVersion::V2,
        ManifestContentType::Data,
    );
    let data_file = DataFile {
        content: DataContentType::Data,
        file_path: data_file_path.clone(),
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
        split_offsets: vec![4],
        equality_ids: Vec::new(),
        sort_order_id: None,
        first_row_id: Some(0),
        partition_spec_id: 0,
        referenced_data_file: None,
        content_offset: None,
        content_size_in_bytes: None,
    };
    let mut manifest_writer = ManifestWriterBuilder::new(Some(42), None, manifest_metadata).build();
    manifest_writer.add(data_file);
    let manifest_bytes = manifest_writer.to_avro_bytes_v2().unwrap();
    fs::write(
        Url::parse(&manifest_path).unwrap().to_file_path().unwrap(),
        manifest_bytes,
    )
    .unwrap();
    let delete_manifest_metadata = ManifestMetadata::new(
        Arc::new(table_metadata.current_schema().unwrap().clone()),
        table_metadata.current_schema_id,
        table_metadata.default_partition_spec().unwrap().clone(),
        FormatVersion::V2,
        ManifestContentType::Deletes,
    );
    let delete_file = DataFile {
        content: DataContentType::PositionDeletes,
        file_path: delete_file_path.clone(),
        file_format: DataFileFormat::Parquet,
        partition: Vec::new(),
        record_count: 1,
        file_size_in_bytes: 55,
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
        referenced_data_file: Some(data_file_path.clone()),
        content_offset: None,
        content_size_in_bytes: None,
    };
    let mut delete_manifest_writer =
        ManifestWriterBuilder::new(Some(42), None, delete_manifest_metadata).build();
    delete_manifest_writer.add(delete_file);
    let delete_manifest_bytes = delete_manifest_writer.to_avro_bytes_v2().unwrap();
    fs::write(
        Url::parse(&delete_manifest_path)
            .unwrap()
            .to_file_path()
            .unwrap(),
        delete_manifest_bytes,
    )
    .unwrap();

    let manifest = ManifestFile::builder()
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
        .unwrap();
    let delete_manifest = ManifestFile::builder()
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
        .unwrap();
    let mut writer = ManifestListWriter::new();
    writer.append(manifest);
    writer.append(delete_manifest);
    let bytes = writer.to_bytes(FormatVersion::V2).unwrap();
    fs::write(&manifest_list_path, bytes).unwrap();

    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );

    let plan = engine
        .plan_scan(ScanPlanningRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location.clone()),
            table_metadata: metadata.clone(),
            projection: vec!["id".to_string()],
            filters: Vec::new(),
            limit: None,
            snapshot_id: Some(42),
            start_snapshot_id: None,
            end_snapshot_id: None,
        })
        .await
        .expect("scan planning should produce a manifest-list plan task");
    let plan_task = plan.scan_tasks[0]["plan-task"]
        .as_str()
        .unwrap()
        .to_string();

    let fetched = engine
        .fetch_scan_tasks(FetchScanTasksRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location.clone()),
            table_metadata: metadata.clone(),
            plan_task,
            required_projection: Vec::new(),
            required_filters: Vec::new(),
        })
        .await
        .expect("fetch should expand manifest list through Sail I/O");

    assert_eq!(fetched.plan_tasks.len(), 2);
    assert_eq!(fetched.plan_tasks[0]["task-type"], json!("manifest"));
    assert_eq!(fetched.plan_tasks[0]["manifest-list"], json!(manifest_list));
    assert_eq!(fetched.plan_tasks[0]["manifest-path"], json!(manifest_path));
    assert_eq!(fetched.file_scan_tasks.len(), 1);
    assert_eq!(fetched.delete_files.len(), 1);
    assert_eq!(
        fetched.file_scan_tasks[0].pointer("/delete-file-references/0"),
        Some(&json!(0))
    );
    assert_eq!(
        fetched.delete_files[0].pointer("/content"),
        Some(&json!("position-deletes"))
    );
    assert_eq!(
        fetched.delete_files[0].pointer("/file-path"),
        Some(&json!(delete_file_path))
    );
    let manifest_plan_task = fetched.plan_tasks[0]["plan-task"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        fetched
            .residual_filter
            .unwrap()
            .pointer("/lakecat:sail-target"),
        Some(&json!("sail_iceberg::io::load_manifest_list"))
    );

    let manifest_fetched = engine
        .fetch_scan_tasks(FetchScanTasksRequest {
            table,
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location),
            table_metadata: metadata,
            plan_task: manifest_plan_task,
            required_projection: Vec::new(),
            required_filters: Vec::new(),
        })
        .await
        .expect("fetch should expand manifest through Sail I/O");

    assert_eq!(manifest_fetched.plan_tasks.len(), 0);
    assert_eq!(manifest_fetched.delete_files.len(), 0);
    assert_eq!(manifest_fetched.file_scan_tasks.len(), 1);
    assert_eq!(
        manifest_fetched.file_scan_tasks[0].pointer("/data-file/file-path"),
        Some(&json!(data_file_path))
    );
    assert_eq!(
        manifest_fetched.file_scan_tasks[0].pointer("/data-file/file-format"),
        Some(&json!("parquet"))
    );
    assert_eq!(
        manifest_fetched.file_scan_tasks[0].pointer("/data-file/split-offsets/0"),
        Some(&json!(4))
    );

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn preserves_filter_context_and_prunes_loaded_file_bounds() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-filter-prune-{unique}"));
    let table_dir = root.join("table");
    let metadata_dir = table_dir.join("metadata");
    fs::create_dir_all(&metadata_dir).unwrap();

    let manifest_list_path = metadata_dir.join("snap-42.avro");
    let manifest_path = Url::from_file_path(metadata_dir.join("manifest-1.avro"))
        .unwrap()
        .to_string();
    let table_location = Url::from_file_path(&table_dir).unwrap().to_string();
    let metadata_location = Url::from_file_path(metadata_dir.join("00000.json"))
        .unwrap()
        .to_string();
    let manifest_list = Url::from_file_path(&manifest_list_path)
        .unwrap()
        .to_string();
    let metadata = sample_metadata_with_locations(3, table_location, manifest_list.clone());
    let table_metadata = TableMetadata::from_json(&serde_json::to_vec(&metadata).unwrap()).unwrap();
    let manifest_metadata = ManifestMetadata::new(
        Arc::new(table_metadata.current_schema().unwrap().clone()),
        table_metadata.current_schema_id,
        table_metadata.default_partition_spec().unwrap().clone(),
        FormatVersion::V2,
        ManifestContentType::Data,
    );
    let mut manifest_writer = ManifestWriterBuilder::new(Some(42), None, manifest_metadata).build();
    let filter = json!({
        "type": "eq",
        "term": "id",
        "value": "evt-1"
    });
    for id in ["evt-1", "evt-2"] {
        let mut lower_bounds = HashMap::new();
        lower_bounds.insert(
            1,
            Datum::new(
                PrimitiveType::String,
                PrimitiveLiteral::String(id.to_string()),
            ),
        );
        let upper_bounds = lower_bounds.clone();
        let data_file = DataFile {
            content: DataContentType::Data,
            file_path: Url::from_file_path(table_dir.join("data").join(format!("{id}.parquet")))
                .unwrap()
                .to_string(),
            file_format: DataFileFormat::Parquet,
            partition: Vec::new(),
            record_count: 3,
            file_size_in_bytes: 123,
            column_sizes: HashMap::new(),
            value_counts: HashMap::new(),
            null_value_counts: HashMap::new(),
            nan_value_counts: HashMap::new(),
            lower_bounds,
            upper_bounds,
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
        };
        assert_eq!(
            data_file_may_match_filter(&table_metadata, &data_file, &filter).unwrap(),
            id == "evt-1"
        );
        manifest_writer.add(data_file);
    }
    fs::write(
        Url::parse(&manifest_path).unwrap().to_file_path().unwrap(),
        manifest_writer.to_avro_bytes_v2().unwrap(),
    )
    .unwrap();
    let parsed_manifest = sail_iceberg::spec::Manifest::parse_avro(
        &fs::read(Url::parse(&manifest_path).unwrap().to_file_path().unwrap()).unwrap(),
    )
    .expect("manifest bounds should round-trip through Sail Avro");
    assert_eq!(
        parsed_manifest
            .entries()
            .first()
            .unwrap()
            .data_file
            .lower_bounds()
            .get(&1)
            .unwrap()
            .literal,
        PrimitiveLiteral::String("evt-1".to_string())
    );

    let manifest = ManifestFile::builder()
        .with_manifest_path(&manifest_path)
        .with_manifest_length(10)
        .with_partition_spec_id(0)
        .with_content(ManifestContentType::Data)
        .with_sequence_number(7)
        .with_min_sequence_number(7)
        .with_added_snapshot_id(42)
        .with_file_counts(2, 0, 0)
        .with_row_counts(6, 0, 0)
        .build()
        .unwrap();
    let mut writer = ManifestListWriter::new();
    writer.append(manifest);
    fs::write(
        &manifest_list_path,
        writer.to_bytes(FormatVersion::V2).unwrap(),
    )
    .unwrap();

    let engine = SailRestModelCatalogEngine;
    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let plan = engine
        .plan_scan(ScanPlanningRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location.clone()),
            table_metadata: metadata.clone(),
            projection: vec!["id".to_string()],
            filters: vec![filter],
            limit: None,
            snapshot_id: Some(42),
            start_snapshot_id: None,
            end_snapshot_id: None,
        })
        .await
        .expect("filtered scan planning should validate");
    assert!(
        plan.scan_tasks[0]["plan-task"]
            .as_str()
            .unwrap()
            .starts_with("lakecat:sail-json-hmac:")
    );
    let decoded = decode_plan_task(plan.scan_tasks[0]["plan-task"].as_str().unwrap())
        .expect("structured plan task should decode");
    assert_eq!(decoded.table.as_deref(), Some(table.stable_id().as_str()));
    assert_eq!(decoded.projection, vec!["id".to_string()]);
    assert_eq!(decoded.filters.len(), 1);
    let plan_task = plan.scan_tasks[0]["plan-task"].as_str().unwrap();
    let mut tampered_plan_task = plan_task.to_string();
    let signature_start = "lakecat:sail-json-hmac:".len();
    let replacement = if &tampered_plan_task[signature_start..signature_start + 1] == "0" {
        "1"
    } else {
        "0"
    };
    tampered_plan_task.replace_range(signature_start..signature_start + 1, replacement);
    assert!(
        decode_plan_task(&tampered_plan_task)
            .expect_err("tampered plan task should fail signature verification")
            .to_string()
            .contains("signature")
    );
    let other_table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("other_events").unwrap(),
    );
    let rejected = engine
        .fetch_scan_tasks(FetchScanTasksRequest {
            table: other_table,
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location.clone()),
            table_metadata: metadata.clone(),
            plan_task: plan.scan_tasks[0]["plan-task"]
                .as_str()
                .unwrap()
                .to_string(),
            required_projection: Vec::new(),
            required_filters: Vec::new(),
        })
        .await
        .expect_err("structured plan tasks should be bound to the planned table");
    assert!(
        rejected
            .to_string()
            .contains("does not match requested table")
    );
    let rejected = engine
        .fetch_scan_tasks(FetchScanTasksRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location.clone()),
            table_metadata: metadata.clone(),
            plan_task: format!("lakecat:sail:manifest-list:42:{manifest_list}"),
            required_projection: vec!["id".to_string()],
            required_filters: Vec::new(),
        })
        .await
        .expect_err("legacy plan task should not satisfy a governed projection");
    assert!(
        rejected
            .to_string()
            .contains("required governed projection")
    );

    let fetched = engine
        .fetch_scan_tasks(FetchScanTasksRequest {
            table,
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location),
            table_metadata: metadata,
            plan_task: plan.scan_tasks[0]["plan-task"]
                .as_str()
                .unwrap()
                .to_string(),
            required_projection: vec!["id".to_string()],
            required_filters: vec![json!({
                "type": "eq",
                "term": "id",
                "value": "evt-1"
            })],
        })
        .await
        .expect("fetch should prune files with Sail metadata bounds");
    assert_eq!(fetched.file_scan_tasks.len(), 1);
    assert!(
        fetched.file_scan_tasks[0]["data-file"]["file-path"]
            .as_str()
            .unwrap()
            .ends_with("/evt-1.parquet")
    );

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn plans_incremental_append_chain_with_sail_manifest_io() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("lakecat-incremental-{unique}"));
    let table_dir = root.join("table");
    let metadata_dir = table_dir.join("metadata");
    fs::create_dir_all(&metadata_dir).unwrap();

    let manifest_list_path = metadata_dir.join("snap-42.avro");
    let manifest_path = Url::from_file_path(metadata_dir.join("manifest-42.avro"))
        .unwrap()
        .to_string();
    let delete_manifest_path = Url::from_file_path(metadata_dir.join("delete-manifest-42.avro"))
        .unwrap()
        .to_string();
    let data_file_path = Url::from_file_path(table_dir.join("data").join("part-42.parquet"))
        .unwrap()
        .to_string();
    let delete_file_path = Url::from_file_path(
        table_dir
            .join("deletes")
            .join("part-42-pos-deletes.parquet"),
    )
    .unwrap()
    .to_string();
    let table_location = Url::from_file_path(&table_dir).unwrap().to_string();
    let metadata_location = Url::from_file_path(metadata_dir.join("00001.json"))
        .unwrap()
        .to_string();
    let manifest_list = Url::from_file_path(&manifest_list_path)
        .unwrap()
        .to_string();
    let metadata = json!({
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
                "required": true
            }]
        }],
        "current-schema-id": 1,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 41,
            "sequence-number": 7,
            "timestamp-ms": 1709999999000_i64,
            "manifest-list": "file:///tmp/lakecat-parent-not-loaded.avro",
            "summary": {"operation": "append"},
            "schema-id": 1
        }, {
            "snapshot-id": 42,
            "parent-snapshot-id": 41,
            "sequence-number": 8,
            "timestamp-ms": 1710000000000_i64,
            "manifest-list": manifest_list,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{
            "timestamp-ms": 1709999999000_i64,
            "snapshot-id": 41
        }, {
            "timestamp-ms": 1710000000000_i64,
            "snapshot-id": 42
        }]
    });
    let table_metadata = TableMetadata::from_json(&serde_json::to_vec(&metadata).unwrap()).unwrap();
    let manifest_metadata = ManifestMetadata::new(
        Arc::new(table_metadata.current_schema().unwrap().clone()),
        table_metadata.current_schema_id,
        table_metadata.default_partition_spec().unwrap().clone(),
        FormatVersion::V2,
        ManifestContentType::Data,
    );
    let mut manifest_writer = ManifestWriterBuilder::new(Some(42), None, manifest_metadata).build();
    manifest_writer.add(DataFile {
        content: DataContentType::Data,
        file_path: data_file_path.clone(),
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
        manifest_writer.to_avro_bytes_v2().unwrap(),
    )
    .unwrap();

    let delete_manifest_metadata = ManifestMetadata::new(
        Arc::new(table_metadata.current_schema().unwrap().clone()),
        table_metadata.current_schema_id,
        table_metadata.default_partition_spec().unwrap().clone(),
        FormatVersion::V2,
        ManifestContentType::Deletes,
    );
    let mut delete_manifest_writer =
        ManifestWriterBuilder::new(Some(42), None, delete_manifest_metadata).build();
    delete_manifest_writer.add(DataFile {
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
        referenced_data_file: Some(data_file_path.clone()),
        content_offset: None,
        content_size_in_bytes: None,
    });
    std::fs::write(
        Url::parse(&delete_manifest_path)
            .unwrap()
            .to_file_path()
            .unwrap(),
        delete_manifest_writer.to_avro_bytes_v2().unwrap(),
    )
    .unwrap();

    let mut list_writer = ManifestListWriter::new();
    list_writer.append(
        ManifestFile::builder()
            .with_manifest_path("file:///tmp/lakecat-inherited-not-loaded.avro")
            .with_manifest_length(10)
            .with_partition_spec_id(0)
            .with_content(ManifestContentType::Data)
            .with_sequence_number(7)
            .with_min_sequence_number(7)
            .with_added_snapshot_id(41)
            .with_file_counts(0, 1, 0)
            .with_row_counts(0, 3, 0)
            .build()
            .unwrap(),
    );
    list_writer.append(
        ManifestFile::builder()
            .with_manifest_path(&manifest_path)
            .with_manifest_length(10)
            .with_partition_spec_id(0)
            .with_content(ManifestContentType::Data)
            .with_sequence_number(8)
            .with_min_sequence_number(8)
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
    fs::write(
        &manifest_list_path,
        list_writer.to_bytes(FormatVersion::V2).unwrap(),
    )
    .unwrap();

    let table = TableIdent::new(
        WarehouseName::new("local").unwrap(),
        "default".parse::<Namespace>().unwrap(),
        TableName::new("events").unwrap(),
    );
    let engine = SailRestModelCatalogEngine;
    let plan = engine
        .plan_scan(ScanPlanningRequest {
            table: table.clone(),
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location.clone()),
            table_metadata: metadata.clone(),
            projection: vec!["id".to_string()],
            filters: Vec::new(),
            limit: None,
            snapshot_id: None,
            start_snapshot_id: Some(41),
            end_snapshot_id: Some(42),
        })
        .await
        .expect("incremental append planning should use Sail manifest-list I/O");

    assert_eq!(plan.snapshot_id, Some(42));
    assert_eq!(plan.scan_tasks.len(), 1);
    assert_eq!(
        plan.scan_tasks[0].pointer("/task-type"),
        Some(&json!("incremental-manifest-list"))
    );
    assert_eq!(
        plan.scan_tasks[0].pointer("/manifest-list"),
        Some(&json!(manifest_list))
    );
    assert_eq!(
        plan.residual_filter.as_ref().unwrap().pointer("/scan-mode"),
        Some(&json!("incremental"))
    );

    let fetched = engine
        .fetch_scan_tasks(FetchScanTasksRequest {
            table,
            principal: Principal::anonymous(),
            metadata_location: Some(metadata_location),
            table_metadata: metadata,
            plan_task: plan.scan_tasks[0]["plan-task"]
                .as_str()
                .unwrap()
                .to_string(),
            required_projection: Vec::new(),
            required_filters: Vec::new(),
        })
        .await
        .expect("incremental manifest-list task should expand through Sail I/O");
    assert_eq!(fetched.plan_tasks.len(), 2);
    assert_eq!(
        fetched.file_scan_tasks[0].pointer("/data-file/file-path"),
        Some(&json!(data_file_path))
    );
    assert_eq!(
        fetched.file_scan_tasks[0].pointer("/delete-file-references/0"),
        Some(&json!(0))
    );
    assert_eq!(
        fetched.delete_files[0].pointer("/file-path"),
        Some(&json!(delete_file_path))
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn encodes_delete_files_as_generated_iceberg_rest_models() {
    let delete_file = DataFile {
        content: DataContentType::PositionDeletes,
        file_path: "file:///tmp/events/delete-1.parquet".to_string(),
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
        referenced_data_file: Some("file:///tmp/events/data-1.parquet".to_string()),
        content_offset: Some(10),
        content_size_in_bytes: Some(20),
    };

    let encoded = rest_delete_file_value(&delete_file).unwrap();
    let _: models::PositionDeleteFile = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(encoded["content"], json!("position-deletes"));
    assert_eq!(encoded["content-offset"], json!(10));
    assert_eq!(encoded["content-size-in-bytes"], json!(20));
}

fn sample_metadata(format_version: i32) -> Value {
    sample_metadata_with_locations(
        format_version,
        "file:///tmp/events".to_string(),
        "file:///tmp/events/metadata/snap-42.avro".to_string(),
    )
}

fn sample_metadata_with_locations(
    format_version: i32,
    table_location: String,
    manifest_list: String,
) -> Value {
    json!({
        "format-version": format_version,
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
            "manifest-list": manifest_list,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{
            "timestamp-ms": 1710000000000_i64,
            "snapshot-id": 42
        }]
    })
}
