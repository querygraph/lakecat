use crate::*;

#[cfg(feature = "qglake-fixture")]
pub(crate) fn qglake_table_metadata(
    location: &str,
    metadata_location: &str,
) -> lakecat_core::LakeCatResult<Value> {
    let metadata_file = file_url_path(metadata_location, "QGLake metadata location")?;
    let metadata_dir = metadata_file.parent().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake metadata location has no parent directory: {metadata_location}"
        ))
    })?;
    fs::create_dir_all(metadata_dir).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to create QGLake metadata directory {metadata_dir:?}: {err}"
        ))
    })?;

    let table_dir = file_url_path(location, "QGLake table location")?;
    let data_dir = table_dir.join("data");
    let delete_dir = table_dir.join("delete");
    fs::create_dir_all(&data_dir).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to create QGLake data directory {data_dir:?}: {err}"
        ))
    })?;
    fs::create_dir_all(&delete_dir).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to create QGLake delete directory {delete_dir:?}: {err}"
        ))
    })?;

    let manifest_list_path = metadata_dir.join("snap-42.avro");
    let manifest_path = metadata_dir.join("manifest-1.avro");
    let delete_manifest_path = metadata_dir.join("delete-manifest-1.avro");
    let data_file_path = data_dir.join("part-1.parquet");
    let delete_file_path = delete_dir.join("pos-delete-1.parquet");
    let manifest_list = file_path_url(&manifest_list_path, "QGLake manifest list")?;
    let manifest = file_path_url(&manifest_path, "QGLake data manifest")?;
    let delete_manifest = file_path_url(&delete_manifest_path, "QGLake delete manifest")?;
    let data_file = file_path_url(&data_file_path, "QGLake data file")?;
    let delete_file = file_path_url(&delete_file_path, "QGLake delete file")?;

    let metadata = json!({
        "format-version": 3,
        "table-uuid": "22222222-2222-2222-2222-222222222222",
        "location": location,
        "last-sequence-number": 8,
        "last-updated-ms": 1710000000000_i64,
        "last-column-id": 4,
        "current-schema-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 1,
            "fields": [
                {
                    "id": 1,
                    "name": "event_id",
                    "type": "string",
                    "required": true,
                    "doc": "Event identifier.",
                    "semantic-type": "https://schema.org/identifier"
                },
                {
                    "id": 2,
                    "name": "occurred_at",
                    "type": "timestamp",
                    "required": false,
                    "doc": "Event timestamp.",
                    "semantic-type": "https://schema.org/DateTime"
                },
                {
                    "id": 3,
                    "name": "severity",
                    "type": "string",
                    "required": false,
                    "doc": "Operational severity."
                },
                {
                    "id": 4,
                    "name": "raw_payload",
                    "type": "string",
                    "required": false,
                    "doc": "Raw event payload reserved for governed human/debug workflows."
                }
            ]
        }],
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": 42,
        "snapshots": [{
            "snapshot-id": 42,
            "sequence-number": 8,
            "timestamp-ms": 1710000000000_i64,
            "manifest-list": manifest_list,
            "summary": {"operation": "append"},
            "schema-id": 1
        }],
        "snapshot-log": [{
            "timestamp-ms": 1710000000000_i64,
            "snapshot-id": 42
        }]
    });

    write_qglake_manifest_files(
        &metadata,
        &manifest_path,
        &manifest,
        &delete_manifest_path,
        &delete_manifest,
        &manifest_list_path,
        &data_file,
        &delete_file,
    )?;
    write_qglake_metadata_file(&metadata_file, &metadata)?;
    Ok(metadata)
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn write_qglake_metadata_file(
    metadata_file: &std::path::Path,
    metadata: &Value,
) -> lakecat_core::LakeCatResult<()> {
    let bytes = serde_json::to_vec_pretty(metadata).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to encode QGLake table metadata JSON: {err}"
        ))
    })?;
    fs::write(metadata_file, bytes).map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write QGLake table metadata {metadata_file:?}: {err}"
        ))
    })
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn write_qglake_manifest_files(
    metadata: &Value,
    manifest_path: &std::path::Path,
    manifest: &str,
    delete_manifest_path: &std::path::Path,
    delete_manifest: &str,
    manifest_list_path: &std::path::Path,
    data_file: &str,
    delete_file: &str,
) -> lakecat_core::LakeCatResult<()> {
    let table_metadata =
        TableMetadata::from_json(&serde_json::to_vec(metadata).map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to encode QGLake table metadata for manifest writer: {err}"
            ))
        })?)
        .map_err(|err| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake fixture metadata is not valid Iceberg metadata: {err}"
            ))
        })?;
    let manifest_metadata = ManifestMetadata::new(
        Arc::new(
            table_metadata
                .current_schema()
                .ok_or_else(|| {
                    lakecat_core::LakeCatError::InvalidArgument(
                        "QGLake fixture metadata has no current schema".to_string(),
                    )
                })?
                .clone(),
        ),
        table_metadata.current_schema_id,
        table_metadata
            .default_partition_spec()
            .ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(
                    "QGLake fixture metadata has no default partition spec".to_string(),
                )
            })?
            .clone(),
        FormatVersion::V2,
        ManifestContentType::Data,
    );
    let mut manifest_writer = ManifestWriterBuilder::new(Some(42), None, manifest_metadata).build();
    manifest_writer.add(DataFile {
        content: DataContentType::Data,
        file_path: data_file.to_string(),
        file_format: DataFileFormat::Parquet,
        partition: Vec::new(),
        record_count: 3,
        file_size_in_bytes: 256,
        column_sizes: Default::default(),
        value_counts: Default::default(),
        null_value_counts: Default::default(),
        nan_value_counts: Default::default(),
        lower_bounds: Default::default(),
        upper_bounds: Default::default(),
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
    fs::write(
        manifest_path,
        manifest_writer.to_avro_bytes_v2().map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to encode QGLake data manifest: {err}"
            ))
        })?,
    )
    .map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write QGLake data manifest {manifest_path:?}: {err}"
        ))
    })?;

    let delete_manifest_metadata = ManifestMetadata::new(
        Arc::new(
            table_metadata
                .current_schema()
                .ok_or_else(|| {
                    lakecat_core::LakeCatError::InvalidArgument(
                        "QGLake fixture metadata has no current schema".to_string(),
                    )
                })?
                .clone(),
        ),
        table_metadata.current_schema_id,
        table_metadata
            .default_partition_spec()
            .ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(
                    "QGLake fixture metadata has no default partition spec".to_string(),
                )
            })?
            .clone(),
        FormatVersion::V2,
        ManifestContentType::Deletes,
    );
    let mut delete_writer =
        ManifestWriterBuilder::new(Some(42), None, delete_manifest_metadata).build();
    delete_writer.add(DataFile {
        content: DataContentType::PositionDeletes,
        file_path: delete_file.to_string(),
        file_format: DataFileFormat::Parquet,
        partition: Vec::new(),
        record_count: 1,
        file_size_in_bytes: 64,
        column_sizes: Default::default(),
        value_counts: Default::default(),
        null_value_counts: Default::default(),
        nan_value_counts: Default::default(),
        lower_bounds: Default::default(),
        upper_bounds: Default::default(),
        block_size_in_bytes: None,
        key_metadata: None,
        split_offsets: Vec::new(),
        equality_ids: Vec::new(),
        sort_order_id: None,
        first_row_id: None,
        partition_spec_id: 0,
        referenced_data_file: Some(data_file.to_string()),
        content_offset: None,
        content_size_in_bytes: None,
    });
    fs::write(
        delete_manifest_path,
        delete_writer.to_avro_bytes_v2().map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to encode QGLake delete manifest: {err}"
            ))
        })?,
    )
    .map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write QGLake delete manifest {delete_manifest_path:?}: {err}"
        ))
    })?;

    let mut list_writer = ManifestListWriter::new();
    list_writer.append(
        ManifestFile::builder()
            .with_manifest_path(manifest)
            .with_manifest_length(10)
            .with_partition_spec_id(0)
            .with_content(ManifestContentType::Data)
            .with_sequence_number(7)
            .with_min_sequence_number(7)
            .with_added_snapshot_id(42)
            .with_file_counts(1, 0, 0)
            .with_row_counts(3, 0, 0)
            .build()
            .map_err(|err| {
                lakecat_core::LakeCatError::Internal(format!(
                    "failed to build QGLake manifest-list entry: {err}"
                ))
            })?,
    );
    list_writer.append(
        ManifestFile::builder()
            .with_manifest_path(delete_manifest)
            .with_manifest_length(10)
            .with_partition_spec_id(0)
            .with_content(ManifestContentType::Deletes)
            .with_sequence_number(8)
            .with_min_sequence_number(8)
            .with_added_snapshot_id(42)
            .with_file_counts(1, 0, 0)
            .with_row_counts(1, 0, 0)
            .build()
            .map_err(|err| {
                lakecat_core::LakeCatError::Internal(format!(
                    "failed to build QGLake delete manifest-list entry: {err}"
                ))
            })?,
    );
    fs::write(
        manifest_list_path,
        list_writer.to_bytes(FormatVersion::V2).map_err(|err| {
            lakecat_core::LakeCatError::Internal(format!(
                "failed to encode QGLake manifest list: {err}"
            ))
        })?,
    )
    .map_err(|err| {
        lakecat_core::LakeCatError::Internal(format!(
            "failed to write QGLake manifest list {manifest_list_path:?}: {err}"
        ))
    })?;
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn file_url_path(value: &str, label: &str) -> lakecat_core::LakeCatResult<PathBuf> {
    Url::parse(value)
        .map_err(|err| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be a file URL for local fixture generation: {value}: {err}"
            ))
        })?
        .to_file_path()
        .map_err(|_| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be a file URL for local fixture generation: {value}"
            ))
        })
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn file_path_url(
    path: &std::path::Path,
    label: &str,
) -> lakecat_core::LakeCatResult<String> {
    Url::from_file_path(path)
        .map_err(|_| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} path cannot be converted to a file URL: {path:?}"
            ))
        })
        .map(|url| url.to_string())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn qglake_odrl_policy(table: &str) -> Value {
    json!({
        "@context": {
            "odrl": "http://www.w3.org/ns/odrl/2/",
            "lakecat": "https://querygraph.ai/lakecat/ns#",
            "typesec": "https://typesec.ai/ns#"
        },
        "uid": format!("lakecat:qglake:{table}:agent-read"),
        "type": "odrl:Set",
        "lakecat:read-restriction": {
            "allowed-columns": ["event_id", "occurred_at", "severity"],
            "row-predicate": {
                "type": "not-eq",
                "term": "severity",
                "value": "debug"
            },
            "purpose": "qglake-agent-demo",
            "max-credential-ttl-seconds": 300
        },
        "permission": [{
            "target": table,
            "action": "odrl:read",
            "constraint": [{
                "leftOperand": "typesec:capability",
                "operator": "odrl:eq",
                "rightOperand": "catalog.table.plan_scan"
            }, {
                "leftOperand": "purpose",
                "operator": "odrl:eq",
                "rightOperand": "qglake-agent-demo"
            }]
        }]
    })
}
