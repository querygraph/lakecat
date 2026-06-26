use crate::*;

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn qglake_fixture(
    catalog: String,
    warehouse: String,
    namespace: Vec<String>,
    table: String,
    location: String,
    metadata_location: String,
    output: PathBuf,
    drain_output: Option<PathBuf>,
    principal: Option<String>,
) -> lakecat_core::LakeCatResult<()> {
    let principal = principal.as_deref();
    let identity_mode = RequestIdentityMode::AgentDid;
    let namespace_path = namespace.join(".");
    let server = "lakecat-local";
    let project = "default";
    let storage_profile = format!("{table}-local");
    let policy = format!("{table}-agent-read");

    let _: ServerResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/servers/{server}"),
        principal,
        identity_mode,
        "server upsert",
        &UpsertServerRequest {
            display_name: Some("Local LakeCat".to_string()),
            endpoint_url: Some(catalog.clone()),
            properties: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;
    verify_qglake_server_list(&catalog, server, principal, identity_mode).await?;

    let _: ProjectResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/projects/{project}"),
        principal,
        identity_mode,
        "project upsert",
        &UpsertProjectRequest {
            server_id: Some(server.to_string()),
            display_name: Some("QGLake Project".to_string()),
            properties: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;
    verify_qglake_project_list(&catalog, project, principal, identity_mode).await?;

    let _: WarehouseResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/warehouses/{warehouse}"),
        principal,
        identity_mode,
        "warehouse upsert",
        &UpsertWarehouseRequest {
            project_id: Some(project.to_string()),
            storage_root: Some(location.clone()),
            properties: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;
    verify_qglake_warehouse_list(&catalog, &warehouse, principal, identity_mode).await?;

    ensure_qglake_namespace(&catalog, &namespace, principal, identity_mode).await?;
    ensure_qglake_table(
        &catalog,
        &namespace_path,
        &namespace,
        &table,
        &location,
        &metadata_location,
        principal,
        identity_mode,
    )
    .await?;
    verify_qglake_table_commit_history(
        &catalog,
        &warehouse,
        &namespace_path,
        &namespace,
        &table,
        principal,
        identity_mode,
    )
    .await?;
    let _: StorageProfileResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/warehouses/{warehouse}/storage-profiles/{storage_profile}"),
        principal,
        identity_mode,
        "storage profile upsert",
        &UpsertStorageProfileRequest {
            location_prefix: location.clone(),
            provider: "file".to_string(),
            issuance_mode: "local-file-no-secret".to_string(),
            secret_ref: None,
            public_config: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;

    verify_qglake_storage_profile_list(
        &catalog,
        &warehouse,
        &storage_profile,
        principal,
        identity_mode,
    )
    .await?;

    let _: PolicyBindingResponse = put_json_with_identity(
        &catalog,
        &format!("/management/v1/warehouses/{warehouse}/policies/{policy}"),
        principal,
        identity_mode,
        "policy upsert",
        &UpsertPolicyBindingRequest {
            namespace: Some(namespace.clone()),
            table: Some(table.clone()),
            enforced: true,
            odrl: qglake_odrl_policy(&table),
        },
    )
    .await?;
    verify_qglake_policy_list(&catalog, &warehouse, &policy, principal, identity_mode).await?;

    verify_qglake_governed_scan(
        &catalog,
        &namespace_path,
        &table,
        &location,
        principal,
        identity_mode,
    )
    .await?;
    verify_qglake_credentials_blocked(&catalog, &namespace_path, &table, principal, identity_mode)
        .await?;
    verify_qglake_trusted_human_credentials(&catalog, &namespace_path, &table, &location).await?;
    let view = "active_customers_view";
    let view_version = ensure_qglake_transient_view(
        &catalog,
        &warehouse,
        &namespace_path,
        &namespace,
        view,
        &table,
        principal,
        identity_mode,
    )
    .await?;
    let (bundle, verification) =
        fetch_bootstrap_bundle_with_identity(&catalog, principal, identity_mode).await?;
    verify_qglake_bootstrap_bundle(&bundle, &namespace, &table)?;
    write_bootstrap_bundle(&output, &bundle, &verification)?;
    drop_qglake_transient_view(
        &catalog,
        &warehouse,
        &namespace_path,
        view,
        view_version,
        principal,
        identity_mode,
    )
    .await?;
    verify_qglake_view_receipt_chains(
        &catalog,
        &warehouse,
        &namespace_path,
        &namespace,
        view,
        principal,
        identity_mode,
    )
    .await?;
    let drain = drain_lineage_outbox_with_identity(&catalog, principal, identity_mode).await?;
    if let Some(drain_output) = drain_output.as_ref() {
        write_json_file(drain_output, &drain, "lineage drain response")?;
        println!("wrote lineage drain response to {}", drain_output.display());
    }
    verify_qglake_lineage_drain(
        &drain,
        &verification,
        principal,
        qglake_policy_binding_count(&bundle),
    )?;
    println!("drained {} lineage/outbox event(s)", drain.delivered);
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn ensure_qglake_namespace(
    catalog: &str,
    namespace: &[String],
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    if post_json_or_conflict_with_identity::<_, NamespaceResponse>(
        catalog,
        "/catalog/v1/namespaces",
        principal,
        identity_mode,
        "namespace create",
        &CreateNamespaceRequest {
            namespace: namespace.to_vec(),
        },
    )
    .await?
    .is_some()
    {
        return Ok(());
    }

    let namespaces = get_json_with_identity::<ListNamespacesResponse>(
        catalog,
        "/catalog/v1/namespaces",
        principal,
        identity_mode,
        "namespace list",
    )
    .await?;
    if namespace_list_contains(&namespaces, namespace) {
        return Ok(());
    }
    Err(lakecat_core::LakeCatError::InvalidArgument(format!(
        "namespace create conflicted but {} was not present in namespace list",
        namespace.join(".")
    )))
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn ensure_qglake_table(
    catalog: &str,
    namespace_path: &str,
    namespace: &[String],
    table: &str,
    location: &str,
    metadata_location: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = post_json_or_conflict_with_identity::<_, LoadTableResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables"),
        principal,
        identity_mode,
        "table create",
        &CreateTableRequest {
            name: table.to_string(),
            location: Some(location.to_string()),
            schema: None,
            partition_spec: None,
            write_order: None,
            properties: None,
            stage_create: None,
            metadata_location: Some(metadata_location.to_string()),
            metadata: qglake_table_metadata(location, metadata_location)?,
        },
    )
    .await?;
    if let Some(response) = response {
        return verify_qglake_existing_table(&response, namespace, table, metadata_location);
    }

    let response = get_json_with_identity::<LoadTableResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}"),
        principal,
        identity_mode,
        "table load",
    )
    .await?;
    verify_qglake_existing_table(&response, namespace, table, metadata_location)
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn ensure_qglake_transient_view(
    catalog: &str,
    warehouse: &str,
    namespace_path: &str,
    namespace: &[String],
    view: &str,
    table: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<u64> {
    let response = put_json_with_identity::<_, ViewResponse>(
        catalog,
        &format!("/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/views/{view}"),
        principal,
        identity_mode,
        "transient view upsert",
        &UpsertViewRequest {
            sql: format!(
                "select event_id from {}.{} where event_type = 'purchase'",
                namespace.join("."),
                table
            ),
            dialect: "spark-sql".to_string(),
            schema_version: Some(1),
            expected_view_version: None,
            columns: vec![lakecat_api::ViewColumnRequest {
                name: "event_id".to_string(),
                data_type: json!({"type": "long"}),
                nullable: false,
                comment: Some("Projected event identifier".to_string()),
            }],
            properties: BTreeMap::from([("lakecat.fixture".to_string(), "qglake".to_string())]),
        },
    )
    .await?;
    if response.name != view || response.namespace != namespace {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view response did not match fixture target: {} {:?}",
            response.name, response.namespace
        )));
    }
    if response.view_version == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view expected a durable nonzero view version, got {}",
            response.view_version
        )));
    }
    Ok(response.view_version)
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn drop_qglake_transient_view(
    catalog: &str,
    warehouse: &str,
    namespace_path: &str,
    view: &str,
    expected_view_version: u64,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    delete_with_identity(
        catalog,
        &format!(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/views/{view}?expected-view-version={expected_view_version}"
        ),
        principal,
        identity_mode,
        "transient view drop",
    )
    .await?;
    let receipts = get_json_with_identity::<ListViewVersionReceiptsResponse>(
        catalog,
        &format!(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/views/{view}/version-receipts"
        ),
        principal,
        identity_mode,
        "transient view receipt list",
    )
    .await?;
    verify_qglake_transient_view_tombstone_receipts(&receipts, view)
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_view_receipt_chains(
    catalog: &str,
    warehouse: &str,
    namespace_path: &str,
    namespace: &[String],
    view: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let chains = get_json_with_identity::<ListViewVersionReceiptChainsResponse>(
        catalog,
        &format!(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/view-version-receipt-chains"
        ),
        principal,
        identity_mode,
        "transient view receipt-chain list",
    )
    .await?;
    let Some(chain) = chains.chains.iter().find(|chain| {
        chain.warehouse == warehouse && chain.namespace == namespace && chain.name == view
    }) else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake receipt-chain read did not expose transient view {view}"
        )));
    };
    if !chain.tombstoned || chain.latest_operation != "drop" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake receipt-chain read did not expose transient view {view} as tombstoned"
        )));
    }
    if !chain.chain_verified
        || chain.chain_hash.is_empty()
        || chain.latest_view_version == 0
        || chain.receipt_count == 0
        || chain.receipts.is_empty()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake receipt-chain read for transient view {view} is missing a verified chain hash or versioned receipts"
        )));
    }
    let has_drop_receipt = chain.receipts.iter().any(|receipt| {
        receipt.name == view
            && receipt.operation == "drop"
            && !receipt.receipt_hash.is_empty()
            && !receipt.view_hash.is_empty()
    });
    if !has_drop_receipt {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake receipt-chain read for transient view {view} is missing a hashed drop receipt"
        )));
    }
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn verify_qglake_transient_view_tombstone_receipts(
    receipts: &ListViewVersionReceiptsResponse,
    view: &str,
) -> lakecat_core::LakeCatResult<()> {
    let Some(drop_receipt) = receipts
        .receipts
        .iter()
        .find(|receipt| receipt.name == view && receipt.operation == "drop")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view {view} did not expose a drop tombstone receipt"
        )));
    };
    if drop_receipt.receipt_hash.is_empty() || drop_receipt.view_hash.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view {view} tombstone receipt is missing hashes"
        )));
    }
    if drop_receipt.previous_view_version != Some(drop_receipt.view_version) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake transient view {view} tombstone receipt did not preserve the deleted view version"
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn namespace_list_contains(
    response: &ListNamespacesResponse,
    namespace: &[String],
) -> bool {
    response
        .namespaces
        .iter()
        .any(|candidate| candidate == namespace)
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn verify_qglake_existing_table(
    response: &LoadTableResponse,
    namespace: &[String],
    table: &str,
    metadata_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    let expected = TableIdentifier {
        namespace: namespace.to_vec(),
        name: table.to_string(),
    };
    if response.identifier != expected {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table identifier did not match fixture target: {:?}",
            response.identifier
        )));
    }
    if response.metadata_location.as_deref() != Some(metadata_location) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table metadata location did not match fixture target: {:?}",
            response.metadata_location
        )));
    }
    verify_qglake_metadata_pointer(metadata_location, &response.metadata)?;
    if response.metadata["format-version"] != json!(3) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table is not Iceberg v3 fixture metadata: {}",
            response.metadata["format-version"].clone()
        )));
    }
    if !metadata_has_field(&response.metadata, "raw_payload") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "existing QGLake table does not include restricted raw_payload column".to_string(),
        ));
    }
    if response.metadata["current-snapshot-id"].as_i64().is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "existing QGLake table does not include a current snapshot for governed scan planning"
                .to_string(),
        ));
    }
    if !metadata_has_manifest_list(&response.metadata) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "existing QGLake table does not include a snapshot manifest list for fetchScanTasks"
                .to_string(),
        ));
    }
    verify_qglake_manifest_lists(&response.metadata)?;
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn verify_qglake_metadata_pointer(
    metadata_location: &str,
    expected_metadata: &Value,
) -> lakecat_core::LakeCatResult<()> {
    let metadata_file = file_url_path(metadata_location, "QGLake metadata location")?;
    let bytes = fs::read(&metadata_file).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table metadata location is not readable at {metadata_location}: {err}"
        ))
    })?;
    let pointer_metadata = serde_json::from_slice::<Value>(&bytes).map_err(|err| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "existing QGLake table metadata location is not valid JSON at {metadata_location}: {err}"
        ))
    })?;
    if pointer_metadata != *expected_metadata {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "existing QGLake table metadata pointer does not match catalog metadata".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn metadata_has_field(metadata: &Value, field_name: &str) -> bool {
    metadata["schemas"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|schema| schema["fields"].as_array().into_iter().flatten())
        .any(|field| field["name"] == field_name)
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn metadata_has_manifest_list(metadata: &Value) -> bool {
    metadata["snapshots"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|snapshot| snapshot["manifest-list"].as_str().is_some())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn verify_qglake_manifest_lists(metadata: &Value) -> lakecat_core::LakeCatResult<()> {
    for manifest_list in metadata["snapshots"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|snapshot| snapshot["manifest-list"].as_str())
    {
        let manifest_list_file = file_url_path(manifest_list, "QGLake manifest list")?;
        if !manifest_list_file.is_file() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "existing QGLake table manifest list is not readable at {manifest_list}"
            )));
        }
    }
    Ok(())
}
