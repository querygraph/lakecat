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

pub(crate) fn verify_qglake_bootstrap_bundle(
    bundle: &QueryGraphBootstrap,
    namespace: &[String],
    table: &str,
) -> lakecat_core::LakeCatResult<()> {
    let projection = bundle
        .tables
        .iter()
        .find(|candidate| {
            candidate.ident.namespace.parts() == namespace && candidate.ident.name.as_str() == table
        })
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap did not include table {}.{}",
                namespace.join("."),
                table
            ))
        })?;
    verify_qglake_bootstrap_standards(bundle)?;
    verify_qglake_bootstrap_projection(projection, namespace, table)?;
    verify_qglake_bootstrap_graph(bundle, projection)?;
    verify_qglake_bootstrap_open_lineage(bundle, projection)?;
    bundle.verify_manifest()?;
    verify_qglake_querygraph_import_contract(bundle)?;
    Ok(())
}

pub(crate) const QGLAKE_BOOTSTRAP_STANDARDS: &[&str] = &[
    "Iceberg REST",
    "Croissant",
    "CDIF",
    "OSI handoff",
    "ODRL",
    "Grust catalog graph",
    "OpenLineage",
];

pub(crate) fn verify_qglake_bootstrap_standards(
    bundle: &QueryGraphBootstrap,
) -> lakecat_core::LakeCatResult<()> {
    for expected in QGLAKE_BOOTSTRAP_STANDARDS {
        if !bundle
            .manifest
            .standards
            .iter()
            .any(|standard| standard == *expected)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap manifest did not advertise required standard {expected}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_querygraph_import_contract(
    bundle: &QueryGraphBootstrap,
) -> lakecat_core::LakeCatResult<()> {
    let import_contract = bundle.manifest.querygraph_import.as_ref().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "QGLake bootstrap manifest did not include QueryGraph import compatibility evidence"
                .to_string(),
        )
    })?;
    if import_contract.schema_version != "lakecat.querygraph.import-compat.v1" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap QueryGraph import contract used unsupported schema {}",
            import_contract.schema_version
        )));
    }
    if import_contract.table_only_bundle_hash.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "QGLake bootstrap QueryGraph import contract did not include table-only bundle hash"
                .to_string(),
        ));
    }
    if import_contract.graph_hash != bundle.manifest.graph_hash {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap QueryGraph import contract graph hash did not match manifest: {}",
            import_contract.graph_hash
        )));
    }
    if import_contract.view_count != bundle.views.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap QueryGraph import contract view count {} did not match bundle views {}",
            import_contract.view_count,
            bundle.views.len()
        )));
    }
    if bundle.views.is_empty() {
        if !import_contract.view_receipt_evidence.is_empty()
            || import_contract.view_receipt_evidence_hash.is_some()
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap QueryGraph import contract carried view receipt evidence without views"
                    .to_string(),
            ));
        }
    } else {
        if import_contract.view_receipt_evidence.len() != bundle.views.len() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap QueryGraph import contract listed {} view receipt evidence record(s) for {} view(s)",
                import_contract.view_receipt_evidence.len(),
                bundle.views.len()
            )));
        }
        if import_contract
            .view_receipt_evidence_hash
            .as_deref()
            .map_or(true, str::is_empty)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap QueryGraph import contract did not include a view receipt evidence hash"
                    .to_string(),
            ));
        }
        for view in &bundle.views {
            let Some(evidence) = import_contract
                .view_receipt_evidence
                .iter()
                .find(|evidence| evidence.stable_id == view.stable_id)
            else {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "QGLake bootstrap QueryGraph import contract is missing receipt evidence for {}",
                    view.stable_id
                )));
            };
            if evidence.view_version != view.view_version
                || evidence.receipt_hash.is_empty()
                || evidence.receipt_chain_hash.is_empty()
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "QGLake bootstrap QueryGraph import contract receipt evidence for {} did not match the accepted view version and receipt chain",
                    view.stable_id
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_bootstrap_projection(
    projection: &lakecat_querygraph::QueryGraphTableProjection,
    namespace: &[String],
    table: &str,
) -> lakecat_core::LakeCatResult<()> {
    let expected_policy = format!("{table}-agent-read");
    let binding = projection
        .policy_bindings
        .iter()
        .find(|binding| binding.policy_id == expected_policy)
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap table {} did not include policy binding {expected_policy}",
                projection.stable_id
            ))
        })?;
    if !binding.enforced {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy binding {expected_policy} is not enforced"
        )));
    }
    if binding.namespace.as_deref() != Some(namespace) || binding.table.as_deref() != Some(table) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy binding {expected_policy} is not scoped to {}.{table}",
            namespace.join(".")
        )));
    }
    let restriction = &binding.odrl["lakecat:read-restriction"];
    if restriction["allowed-columns"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy allowed columns were not exported as expected: {}",
            restriction["allowed-columns"].clone()
        )));
    }
    if restriction["row-predicate"]
        != json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        })
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy row predicate was not exported as expected: {}",
            restriction["row-predicate"].clone()
        )));
    }
    if restriction["purpose"] != json!("qglake-agent-demo") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy purpose was not exported as expected: {}",
            restriction["purpose"].clone()
        )));
    }
    if restriction["max-credential-ttl-seconds"] != json!(300) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap policy max credential TTL was not exported as expected: {}",
            restriction["max-credential-ttl-seconds"].clone()
        )));
    }
    let embedded_bindings = projection.odrl["lakecat:policy-bindings"]
        .as_array()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap ODRL table projection did not embed policy bindings for {expected_policy}"
            ))
        })?;
    let embedded_binding = embedded_bindings
        .iter()
        .find(|embedded| embedded["policy-id"] == expected_policy)
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap ODRL table projection did not embed {expected_policy}"
            ))
        })?;
    if embedded_binding["odrl"] != binding.odrl {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap embedded ODRL policy binding {expected_policy} drifted from structured policy binding"
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_bootstrap_open_lineage(
    bundle: &QueryGraphBootstrap,
    projection: &lakecat_querygraph::QueryGraphTableProjection,
) -> lakecat_core::LakeCatResult<()> {
    if bundle.open_lineage["eventType"] != json!("COMPLETE") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage eventType was not COMPLETE: {}",
            bundle.open_lineage["eventType"].clone()
        )));
    }
    if bundle.open_lineage["producer"] != json!("https://querygraph.ai/lakecat") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage producer was not LakeCat: {}",
            bundle.open_lineage["producer"].clone()
        )));
    }
    if bundle.open_lineage["schemaURL"]
        != json!("https://openlineage.io/spec/2-0-2/OpenLineage.json")
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage schemaURL was not the expected OpenLineage schema: {}",
            bundle.open_lineage["schemaURL"].clone()
        )));
    }
    if bundle.open_lineage["job"]["name"] != json!("querygraph-bootstrap") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage job name was not querygraph-bootstrap: {}",
            bundle.open_lineage["job"]["name"].clone()
        )));
    }
    let expected_job_namespace = format!("lakecat.{}", bundle.warehouse.as_str());
    if bundle.open_lineage["job"]["namespace"] != json!(expected_job_namespace) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage job namespace did not match warehouse: {}",
            bundle.open_lineage["job"]["namespace"].clone()
        )));
    }
    let semantic_bundle = bundle
        .open_lineage
        .pointer("/run/facets/queryGraph_semanticBundle")
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap OpenLineage did not include queryGraph_semanticBundle facet"
                    .to_string(),
            )
        })?;
    if semantic_bundle["tableCount"] != json!(bundle.tables.len()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage tableCount did not match bundle tables: {}",
            semantic_bundle["tableCount"].clone()
        )));
    }
    if semantic_bundle["viewCount"] != json!(bundle.views.len()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage viewCount did not match bundle views: {}",
            semantic_bundle["viewCount"].clone()
        )));
    }
    for expected in QGLAKE_BOOTSTRAP_STANDARDS {
        let standards = semantic_bundle["standards"].as_array().ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap OpenLineage semantic bundle did not list standards".to_string(),
            )
        })?;
        if !standards
            .iter()
            .any(|standard| standard.as_str() == Some(*expected))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap OpenLineage semantic bundle did not advertise required standard {expected}"
            )));
        }
    }
    if semantic_bundle["graphHash"] != json!(bundle.manifest.graph_hash) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage graph hash did not match manifest: {}",
            semantic_bundle["graphHash"].clone()
        )));
    }
    verify_qglake_open_lineage_artifacts(semantic_bundle, &bundle.manifest)?;
    let output = bundle
        .open_lineage
        .get("outputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|output| {
            output["name"] == projection.ident.name.as_str()
                && output["facets"]["queryGraph_catalog"]["stableId"] == projection.stable_id
        })
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap OpenLineage output did not include {}",
                projection.stable_id
            ))
        })?;
    if output["facets"]["queryGraph_catalog"]["metadataLocation"]
        != json!(projection.metadata_location)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage metadata location did not match table projection: {}",
            output["facets"]["queryGraph_catalog"]["metadataLocation"].clone()
        )));
    }
    if output["facets"]["dataSource"]["uri"] != json!(projection.location) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage data-source URI did not match table location: {}",
            output["facets"]["dataSource"]["uri"].clone()
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_open_lineage_artifacts(
    semantic_bundle: &Value,
    manifest: &lakecat_querygraph::QueryGraphBundleManifest,
) -> lakecat_core::LakeCatResult<()> {
    let table_artifacts = semantic_bundle["tableArtifacts"]
        .as_array()
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap OpenLineage semantic bundle did not list table artifacts"
                    .to_string(),
            )
        })?;
    for manifest_artifact in &manifest.table_artifacts {
        let lineage_artifact = table_artifacts
            .iter()
            .find(|artifact| artifact["stableId"] == manifest_artifact.stable_id)
            .ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "QGLake bootstrap OpenLineage semantic bundle did not include table artifact {}",
                    manifest_artifact.stable_id
                ))
            })?;
        verify_qglake_open_lineage_table_artifact(lineage_artifact, manifest_artifact)?;
    }

    let view_artifacts = semantic_bundle["viewArtifacts"].as_array().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "QGLake bootstrap OpenLineage semantic bundle did not list view artifacts".to_string(),
        )
    })?;
    for manifest_artifact in &manifest.view_artifacts {
        let lineage_artifact = view_artifacts
            .iter()
            .find(|artifact| artifact["stableId"] == manifest_artifact.stable_id)
            .ok_or_else(|| {
                lakecat_core::LakeCatError::InvalidArgument(format!(
                    "QGLake bootstrap OpenLineage semantic bundle did not include view artifact {}",
                    manifest_artifact.stable_id
                ))
            })?;
        verify_qglake_open_lineage_view_artifact(lineage_artifact, manifest_artifact)?;
    }

    Ok(())
}

pub(crate) fn verify_qglake_open_lineage_table_artifact(
    lineage_artifact: &Value,
    manifest_artifact: &lakecat_querygraph::QueryGraphTableArtifactHashes,
) -> lakecat_core::LakeCatResult<()> {
    for (field, expected) in [
        ("croissantHash", manifest_artifact.croissant_hash.as_str()),
        ("cdifHash", manifest_artifact.cdif_hash.as_str()),
        ("osiHash", manifest_artifact.osi_hash.as_str()),
        ("odrlHash", manifest_artifact.odrl_hash.as_str()),
        (
            "policyBindingsHash",
            manifest_artifact.policy_bindings_hash.as_str(),
        ),
    ] {
        if lineage_artifact[field] != json!(expected) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap OpenLineage table artifact {field} did not match manifest: {}",
                lineage_artifact[field].clone()
            )));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_open_lineage_view_artifact(
    lineage_artifact: &Value,
    manifest_artifact: &lakecat_querygraph::QueryGraphViewArtifactHashes,
) -> lakecat_core::LakeCatResult<()> {
    if lineage_artifact["osiHash"] != json!(manifest_artifact.osi_hash) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap OpenLineage view artifact osiHash did not match manifest: {}",
            lineage_artifact["osiHash"].clone()
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_bootstrap_graph(
    bundle: &QueryGraphBootstrap,
    projection: &lakecat_querygraph::QueryGraphTableProjection,
) -> lakecat_core::LakeCatResult<()> {
    let namespace_id = format!(
        "lakecat:namespace:{}:{}",
        projection.ident.warehouse, projection.ident.namespace
    );
    let warehouse_id = format!("lakecat:warehouse:{}", projection.ident.warehouse);
    let table_node = bundle
        .graph
        .nodes
        .iter()
        .find(|node| node.id == projection.stable_id && node.label == "IcebergTable")
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap graph did not include table node {}",
                projection.stable_id
            ))
        })?;
    if table_node.properties["metadataLocation"] != json!(projection.metadata_location) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph metadata location did not match table projection: {}",
            table_node.properties["metadataLocation"].clone()
        )));
    }

    let server_id = bundle
        .graph
        .edges
        .iter()
        .find(|edge| edge.from == "lakecat:catalog" && edge.label == "HAS_SERVER")
        .map(|edge| edge.to.as_str())
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "QGLake bootstrap graph did not connect Catalog to a Server tenant anchor"
                    .to_string(),
            )
        })?;
    let server_node = require_qglake_graph_node_label(bundle, server_id, "Server")?;
    verify_qglake_tenant_root_redaction(server_node, "endpointUrl", "endpointUrlHash", "Server")?;

    let project_id = bundle
        .graph
        .edges
        .iter()
        .find(|edge| edge.from == server_id && edge.label == "HAS_PROJECT")
        .map(|edge| edge.to.as_str())
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap graph did not connect Server {server_id} to a Project tenant anchor"
            ))
        })?;
    require_qglake_graph_node_label(bundle, project_id, "Project")?;

    if !qglake_graph_has_edge(bundle, project_id, &warehouse_id, "HAS_WAREHOUSE") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph did not connect Project {project_id} to warehouse {}",
            projection.ident.warehouse
        )));
    }
    let warehouse_node = require_qglake_graph_node_label(bundle, &warehouse_id, "Warehouse")?;
    verify_qglake_tenant_root_redaction(
        warehouse_node,
        "storageRoot",
        "storageRootHash",
        "Warehouse",
    )?;
    require_qglake_graph_node_label(bundle, &namespace_id, "Namespace")?;

    if !qglake_graph_has_edge(bundle, &warehouse_id, &namespace_id, "HAS_NAMESPACE") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph did not connect warehouse {} to namespace {}",
            projection.ident.warehouse, projection.ident.namespace
        )));
    }

    let has_namespace_edge = bundle
        .graph
        .edges
        .iter()
        .any(|edge| edge.to == projection.stable_id && edge.label == "CONTAINS_TABLE");
    if !has_namespace_edge {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph did not connect namespace to table {}",
            projection.stable_id
        )));
    }
    Ok(())
}

pub(crate) fn require_qglake_graph_node_label<'a>(
    bundle: &'a QueryGraphBootstrap,
    node_id: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a lakecat_querygraph::QueryGraphNode> {
    bundle
        .graph
        .nodes
        .iter()
        .find(|node| node.id == node_id && node.label == label)
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "QGLake bootstrap graph did not include {label} node {node_id}"
            ))
        })
}

pub(crate) fn verify_qglake_tenant_root_redaction(
    node: &lakecat_querygraph::QueryGraphNode,
    raw_field: &str,
    hash_field: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if !node
        .properties
        .get(raw_field)
        .unwrap_or(&Value::Null)
        .is_null()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph {label} node must not expose raw {raw_field}; use {hash_field}"
        )));
    }
    if let Some(hash) = node.properties.get(hash_field)
        && !hash.is_null()
        && !hash.as_str().is_some_and(is_full_sha256_hash)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "QGLake bootstrap graph {label} node {hash_field} must be full SHA-256 hash evidence"
        )));
    }
    Ok(())
}

pub(crate) fn qglake_graph_has_edge(
    bundle: &QueryGraphBootstrap,
    from: &str,
    to: &str,
    label: &str,
) -> bool {
    bundle
        .graph
        .edges
        .iter()
        .any(|edge| edge.from == from && edge.to == to && edge.label == label)
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_policy_list(
    catalog: &str,
    warehouse: &str,
    policy: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListPolicyBindingsResponse>(
        catalog,
        &format!("/management/v1/warehouses/{warehouse}/policies"),
        principal,
        identity_mode,
        "qglake policy list",
    )
    .await?;
    if !response
        .policies
        .iter()
        .any(|binding| binding.policy_id == policy)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake policy list did not return expected binding {policy}"
        )));
    }
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_server_list(
    catalog: &str,
    server: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListServersResponse>(
        catalog,
        "/management/v1/servers",
        principal,
        identity_mode,
        "qglake server list",
    )
    .await?;
    if !response
        .servers
        .iter()
        .any(|candidate| candidate.server_id == server)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake server list did not return expected server {server}"
        )));
    }
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_project_list(
    catalog: &str,
    project: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListProjectsResponse>(
        catalog,
        "/management/v1/projects",
        principal,
        identity_mode,
        "qglake project list",
    )
    .await?;
    if !response
        .projects
        .iter()
        .any(|candidate| candidate.project_id == project)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake project list did not return expected project {project}"
        )));
    }
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_warehouse_list(
    catalog: &str,
    warehouse: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListWarehousesResponse>(
        catalog,
        "/management/v1/warehouses",
        principal,
        identity_mode,
        "qglake warehouse list",
    )
    .await?;
    if !response
        .warehouses
        .iter()
        .any(|candidate| candidate.warehouse == warehouse)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake warehouse list did not return expected warehouse {warehouse}"
        )));
    }
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_storage_profile_list(
    catalog: &str,
    warehouse: &str,
    storage_profile: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let response = get_json_with_identity::<ListStorageProfilesResponse>(
        catalog,
        &format!("/management/v1/warehouses/{warehouse}/storage-profiles"),
        principal,
        identity_mode,
        "qglake storage profile list",
    )
    .await?;
    if !response
        .storage_profiles
        .iter()
        .any(|profile| profile.profile_id == storage_profile)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake storage profile list did not return expected profile {storage_profile}"
        )));
    }
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_table_commit_history(
    catalog: &str,
    warehouse: &str,
    namespace_path: &str,
    namespace: &[String],
    table: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let _: CommitTableResponse = post_json_with_identity_and_idempotency(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/commit"),
        principal,
        identity_mode,
        &format!("qglake:{warehouse}:{namespace_path}:{table}:commit-history"),
        "qglake table commit-history probe commit",
        &CommitTableRequest {
            requirements: Vec::new(),
            updates: Vec::new(),
            metadata_location: None,
            metadata: None,
        },
    )
    .await?;
    let response = get_json_with_identity::<ListTableCommitRecordsResponse>(
        catalog,
        &format!("/management/v1/warehouses/{warehouse}/namespaces/{namespace_path}/tables/{table}/commits"),
        principal,
        identity_mode,
        "qglake table commit history",
    )
    .await?;
    let Some(record) = response.commits.iter().find(|record| {
        record.warehouse == warehouse
            && record.namespace.as_slice() == namespace
            && record.table == table
    }) else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake table commit history did not expose a pointer-log record for {warehouse}.{namespace_path}.{table}"
        )));
    };
    verify_qglake_table_commit_record_evidence(record, warehouse, namespace_path, table)
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_table_commit_record_evidence(
    record: &lakecat_api::TableCommitRecordResponse,
    warehouse: &str,
    namespace_path: &str,
    table: &str,
) -> lakecat_core::LakeCatResult<()> {
    if record.sequence_number == 0
        || record.request_hash.is_empty()
        || record.response_hash.is_empty()
        || record.commit_hash.is_empty()
        || record
            .idempotency_key_sha256
            .as_deref()
            .map_or(true, str::is_empty)
        || record.principal_subject.is_empty()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake table commit history for {warehouse}.{namespace_path}.{table} is missing compact pointer-log evidence"
        )));
    }
    if !is_full_sha256_hash(&record.request_hash)
        || !is_full_sha256_hash(&record.response_hash)
        || !is_full_sha256_hash(&record.commit_hash)
        || !record
            .idempotency_key_sha256
            .as_deref()
            .is_some_and(is_full_sha256_hash)
        || record
            .policy_hash
            .as_deref()
            .is_some_and(|hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake table commit history for {warehouse}.{namespace_path}.{table} must expose full SHA-256 pointer-log hash evidence"
        )));
    }
    if record.format_version != Some(3) || record.snapshot_id.is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake table commit history for {warehouse}.{namespace_path}.{table} is missing Iceberg format/snapshot summary evidence"
        )));
    }
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_governed_scan(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    table_location: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let plan = post_json_with_identity::<_, PlanTableScanResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/plan"),
        principal,
        identity_mode,
        "qglake governed scan plan",
        &PlanTableScanRequest {
            select: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
                "raw_payload".to_string(),
            ],
            stats_fields: vec![
                "event_id".to_string(),
                "occurred_at".to_string(),
                "severity".to_string(),
                "raw_payload".to_string(),
            ],
            case_sensitive: Some(true),
            ..empty_scan_request()
        },
    )
    .await?;
    verify_qglake_scan_plan(&plan)?;
    verify_qglake_fetch_scan_tasks(
        catalog,
        namespace_path,
        table,
        table_location,
        principal,
        identity_mode,
        &plan,
    )
    .await
}

#[cfg(feature = "qglake-fixture")]
pub(crate) fn empty_scan_request() -> PlanTableScanRequest {
    PlanTableScanRequest {
        projection: Vec::new(),
        select: Vec::new(),
        filters: Vec::new(),
        filter: None,
        limit: None,
        snapshot_id: None,
        case_sensitive: None,
        use_snapshot_schema: None,
        start_snapshot_id: None,
        end_snapshot_id: None,
        stats_fields: Vec::new(),
    }
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_scan_plan(
    plan: &PlanTableScanResponse,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_sail_planner("scan plan", &plan.planned_by)?;
    if plan.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed scan plan did not expose an Iceberg REST plan-task token".to_string(),
        ));
    }
    if !plan
        .lakecat_plan_tasks
        .iter()
        .any(|task| task["task-type"] == json!("manifest-list"))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed scan plan did not include a manifest-list task".to_string(),
        ));
    }
    let extension = plan
        .residual_filter
        .as_ref()
        .and_then(|filter| filter.get("lakecat:scan-request"))
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "qglake governed scan plan did not include lakecat:scan-request".to_string(),
            )
        })?;
    if extension.get("effective-projection")
        != Some(&json!(["event_id", "occurred_at", "severity"]))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed scan effective projection was not narrowed as expected: {}",
            extension
                .get("effective-projection")
                .cloned()
                .unwrap_or(Value::Null)
        )));
    }
    if extension.get("requested-stats-fields")
        != Some(&json!([
            "event_id",
            "occurred_at",
            "severity",
            "raw_payload"
        ]))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed scan requested stats fields were not preserved as expected: {}",
            extension
                .get("requested-stats-fields")
                .cloned()
                .unwrap_or(Value::Null)
        )));
    }
    for field in ["effective-stats-fields", "stats-fields"] {
        if extension.get(field) != Some(&json!(["event_id", "occurred_at", "severity"])) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake governed scan {field} were not narrowed as expected: {}",
                extension.get(field).cloned().unwrap_or(Value::Null)
            )));
        }
    }
    verify_qglake_plan_or_fetch_read_restriction(
        &extension["read-restriction"],
        plan.table.name.as_str(),
        "qglake governed scan",
    )?;
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_sail_planner(
    label: &str,
    planned_by: &str,
) -> lakecat_core::LakeCatResult<()> {
    if planned_by != "sail-rest-models" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed {label} was not planned by Sail REST models: {planned_by}"
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn qglake_policy_hash(table: &str) -> lakecat_core::LakeCatResult<String> {
    content_hash_json(&qglake_odrl_policy(table))
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_fetch_scan_tasks(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    table_location: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
    plan: &PlanTableScanResponse,
) -> lakecat_core::LakeCatResult<()> {
    let plan_task = plan.plan_tasks.first().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed scan plan did not produce a plan-task token for fetch verification"
                .to_string(),
        )
    })?;
    let fetched = post_json_with_identity::<_, FetchScanTasksResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/tasks"),
        principal,
        identity_mode,
        "qglake governed scan task fetch",
        &FetchScanTasksRequest {
            plan_task: plan_task.clone(),
        },
    )
    .await?;
    verify_qglake_scan_tasks(&fetched, table_location)?;
    if fetched.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not produce child plan-task tokens for manifest fetch verification"
                .to_string(),
        ));
    }
    for (index, child_plan_task) in fetched.plan_tasks.iter().enumerate() {
        let manifest_fetched = post_json_with_identity::<_, FetchScanTasksResponse>(
            catalog,
            &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/tasks"),
            principal,
            identity_mode,
            "qglake governed manifest scan task fetch",
            &FetchScanTasksRequest {
                plan_task: child_plan_task.clone(),
            },
        )
        .await?;
        let child_descriptor = fetched.lakecat_plan_tasks.get(index);
        if child_descriptor.and_then(|task| task["content"].as_str()) == Some("deletes") {
            verify_qglake_delete_manifest_scan_tasks(&manifest_fetched, table_location)?;
        } else {
            verify_qglake_leaf_scan_tasks(&manifest_fetched, table_location)?;
        }
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_scan_tasks(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_scan_task_common(fetched, table_location)?;
    if fetched.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not expose a child Iceberg REST plan-task token"
                .to_string(),
        ));
    }
    if !fetched
        .lakecat_plan_tasks
        .iter()
        .any(|task| task["task-type"] == json!("manifest"))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not include a manifest child task".to_string(),
        ));
    }
    if fetched.delete_files.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not expose Iceberg delete-file refs".to_string(),
        ));
    }
    if !fetched.file_scan_tasks.iter().any(|task| {
        task.get("delete-file-references")
            .and_then(Value::as_array)
            .is_some_and(|refs| !refs.is_empty())
    }) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks did not attach delete-file references to data tasks"
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_leaf_scan_tasks(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_scan_task_common(fetched, table_location)?;
    if !fetched.plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed manifest fetchScanTasks unexpectedly exposed {} child plan-task token(s)",
            fetched.plan_tasks.len()
        )));
    }
    if !fetched.lakecat_plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed manifest fetchScanTasks unexpectedly included {} LakeCat child task(s)",
            fetched.lakecat_plan_tasks.len()
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_delete_manifest_scan_tasks(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_sail_planner("delete manifest fetchScanTasks", &fetched.planned_by)?;
    if !fetched.plan_tasks.is_empty() || !fetched.lakecat_plan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed delete manifest fetchScanTasks unexpectedly exposed child tasks"
                .to_string(),
        ));
    }
    if !fetched.file_scan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed delete manifest fetchScanTasks unexpectedly exposed data scan tasks"
                .to_string(),
        ));
    }
    if fetched.delete_files.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed delete manifest fetchScanTasks produced no delete files".to_string(),
        ));
    }
    let table_prefix = format!("{}/", table_location.trim_end_matches('/'));
    for delete_file_path in fetched
        .delete_files
        .iter()
        .filter_map(|file| file.get("file-path").and_then(Value::as_str))
    {
        if !delete_file_path.starts_with(&table_prefix) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake governed delete file escaped table location {table_location}: {delete_file_path}"
            )));
        }
    }
    verify_qglake_fetch_restriction(fetched)?;
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_scan_task_common(
    fetched: &FetchScanTasksResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_sail_planner("fetchScanTasks", &fetched.planned_by)?;
    if fetched.file_scan_tasks.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks produced no file scan tasks".to_string(),
        ));
    }
    let data_file_paths = fetched
        .file_scan_tasks
        .iter()
        .filter_map(|task| task.pointer("/data-file/file-path").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if data_file_paths.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake governed fetchScanTasks produced no data-file file paths".to_string(),
        ));
    }
    let table_prefix = format!("{}/", table_location.trim_end_matches('/'));
    for data_file_path in data_file_paths {
        if !data_file_path.starts_with(&table_prefix) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake governed fetchScanTasks data file escaped table location {table_location}: {data_file_path}"
            )));
        }
    }
    verify_qglake_fetch_restriction(fetched)
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_fetch_restriction(
    fetched: &FetchScanTasksResponse,
) -> lakecat_core::LakeCatResult<()> {
    let extension = fetched
        .residual_filter
        .as_ref()
        .and_then(|filter| filter.get("lakecat:fetch-scan-tasks"))
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(
                "qglake governed fetchScanTasks did not include lakecat:fetch-scan-tasks"
                    .to_string(),
            )
        })?;
    verify_qglake_plan_or_fetch_read_restriction(
        &extension["read-restriction"],
        fetched.table.name.as_str(),
        "qglake governed fetchScanTasks",
    )?;
    if extension["required-projection"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks required projection did not prove re-applied narrowing: {}",
            extension["required-projection"].clone()
        )));
    }
    if extension["effective-projection"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks effective projection did not prove re-applied narrowing: {}",
            extension["effective-projection"].clone()
        )));
    }
    if extension["required-filters"]
        .as_array()
        .and_then(|filters| filters.first())
        != Some(&json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        }))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake governed fetchScanTasks required filters did not prove re-applied row predicate: {}",
            extension["required-filters"].clone()
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_plan_or_fetch_read_restriction(
    restriction: &Value,
    table: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if restriction["allowed-columns"] != json!(["event_id", "occurred_at", "severity"]) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} allowed columns were not narrowed as expected: {}",
            restriction["allowed-columns"].clone()
        )));
    }
    if restriction["row-predicate"]
        != json!({
            "type": "not-eq",
            "term": "severity",
            "value": "debug"
        })
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} row predicate was not enforced as expected: {}",
            restriction["row-predicate"].clone()
        )));
    }
    if restriction["purpose"] != json!("qglake-agent-demo") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} purpose was not preserved as expected: {}",
            restriction["purpose"].clone()
        )));
    }
    if restriction["max-credential-ttl-seconds"] != json!(300) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} max credential TTL was not preserved as expected: {}",
            restriction["max-credential-ttl-seconds"].clone()
        )));
    }
    let expected_policy_hash = qglake_policy_hash(table)?;
    let policy_hashes = restriction["policy-hashes"].as_array().ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} read restriction did not include policy hashes"
        ))
    })?;
    if !policy_hashes
        .iter()
        .any(|hash| hash.as_str() == Some(expected_policy_hash.as_str()))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} did not bind to expected ODRL policy hash {expected_policy_hash}: {}",
            restriction["policy-hashes"].clone()
        )));
    }
    Ok(())
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_credentials_blocked(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    principal: Option<&str>,
    identity_mode: RequestIdentityMode,
) -> lakecat_core::LakeCatResult<()> {
    let credentials = get_json_with_identity::<LoadCredentialsResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/credentials"),
        principal,
        identity_mode,
        "qglake restricted credentials probe",
    )
    .await?;
    verify_qglake_credentials_response(&credentials)
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_credentials_response(
    credentials: &LoadCredentialsResponse,
) -> lakecat_core::LakeCatResult<()> {
    if credentials.storage_credentials.is_empty() {
        return Ok(());
    }
    Err(lakecat_core::LakeCatError::InvalidArgument(format!(
        "qglake restricted table unexpectedly returned {} raw credential set(s)",
        credentials.storage_credentials.len()
    )))
}

#[cfg(feature = "qglake-fixture")]
pub(crate) async fn verify_qglake_trusted_human_credentials(
    catalog: &str,
    namespace_path: &str,
    table: &str,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    let credentials = get_json_with_identity::<LoadCredentialsResponse>(
        catalog,
        &format!("/catalog/v1/namespaces/{namespace_path}/tables/{table}/credentials"),
        Some("human:qglake-operator"),
        RequestIdentityMode::Principal,
        "qglake trusted human credentials probe",
    )
    .await?;
    verify_qglake_trusted_human_credentials_response(&credentials, table_location)
}

#[cfg(any(test, feature = "qglake-fixture"))]
pub(crate) fn verify_qglake_trusted_human_credentials_response(
    credentials: &LoadCredentialsResponse,
    table_location: &str,
) -> lakecat_core::LakeCatResult<()> {
    let Some(credential) = credentials.storage_credentials.first() else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake trusted human credentials probe returned no standard credential set"
                .to_string(),
        ));
    };
    if credential.prefix != table_location {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake trusted human credential prefix did not match table location {table_location}: {}",
            credential.prefix
        )));
    }
    if credential
        .config
        .iter()
        .any(|entry| entry.key.contains("secret") || entry.key.contains("token"))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake trusted human local credentials unexpectedly exposed secret material"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn verify_qglake_lineage_drain(
    drain: &LineageDrainResponse,
    verification: &QueryGraphBootstrapVerification,
    principal: Option<&str>,
    expected_policy_binding_count: usize,
) -> lakecat_core::LakeCatResult<()> {
    if drain.delivered == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain delivered no outbox events".to_string(),
        ));
    }
    if drain.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain emitted no lineage events".to_string(),
        ));
    }
    if drain.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain emitted no graph events".to_string(),
        ));
    }
    if !drain
        .event_types
        .iter()
        .any(|event_type| event_type == "querygraph.bootstrap")
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay querygraph.bootstrap".to_string(),
        ));
    }
    let expected_principal = principal.unwrap_or("anonymous");
    if drain.principal_subject.as_deref() != Some(expected_principal) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain read principal did not match accepted principal {expected_principal}"
        )));
    }
    let expected_principal_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    if drain.principal_kind.as_deref() != Some(expected_principal_kind) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain read principal kind did not match accepted principal kind {expected_principal_kind}"
        )));
    }
    if drain
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read is missing full SHA-256 authorization receipt hash"
                .to_string(),
        ));
    }
    if drain.authorization_receipt_action.as_deref() != Some("lineage-read") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read authorization receipt action must be lineage-read"
                .to_string(),
        ));
    }
    if drain
        .request_identity_source
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read is missing request identity source".to_string(),
        ));
    }
    if drain
        .request_identity_state
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain read is missing request identity attestation state".to_string(),
        ));
    }
    verify_qglake_typedid_hash_pair(
        drain.typedid_envelope_hash.as_deref(),
        drain.typedid_proof_hash.as_deref(),
        "qglake lineage drain read",
    )?;
    let Some(bootstrap) = drain
        .events
        .iter()
        .find(|event| event.event_type == "querygraph.bootstrap")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not expose querygraph.bootstrap replay evidence".to_string(),
        ));
    };
    if bootstrap
        .bundle_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
        || bootstrap
            .graph_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || bootstrap
            .open_lineage_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || bootstrap
            .querygraph_import_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing full SHA-256 QueryGraph hashes"
                .to_string(),
        ));
    }
    if bootstrap.principal_subject.as_deref() != Some(expected_principal) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain replay principal did not match accepted principal {expected_principal}"
        )));
    }
    if bootstrap.principal_kind.as_deref() != Some(expected_principal_kind) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain replay principal kind did not match accepted principal kind {expected_principal_kind}"
        )));
    }
    if bootstrap
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing full SHA-256 authorization receipt hash".to_string(),
        ));
    }
    if bootstrap
        .request_identity_source
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing request identity source".to_string(),
        ));
    }
    if bootstrap
        .request_identity_state
        .as_deref()
        .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing request identity attestation state"
                .to_string(),
        ));
    }
    if principal.is_some() {
        if bootstrap
            .agent_delegation_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence is missing full SHA-256 agent delegation hash".to_string(),
            ));
        }
        if bootstrap
            .agent_summary_signature_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence is missing full SHA-256 agent summary signature hash".to_string(),
            ));
        }
    }
    verify_qglake_typedid_hash_pair(
        bootstrap.typedid_envelope_hash.as_deref(),
        bootstrap.typedid_proof_hash.as_deref(),
        "qglake lineage drain bootstrap replay",
    )?;
    if bootstrap.bundle_hash.as_deref() != Some(verification.bundle_hash.as_str())
        || bootstrap.graph_hash.as_deref() != Some(verification.graph_hash.as_str())
        || bootstrap.open_lineage_hash.as_deref() != Some(verification.open_lineage_hash.as_str())
        || bootstrap.querygraph_import_hash.as_deref()
            != Some(verification.querygraph_import_hash.as_str())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence does not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if bootstrap.table_artifact_count == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence has no QueryGraph table artifacts".to_string(),
        ));
    }
    if bootstrap.table_artifact_count != verification.table_count
        || bootstrap.view_artifact_count != verification.view_count
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay artifact counts do not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if !verification.verified_views.is_empty()
        && (bootstrap.view_version_receipt_hashes.len() != verification.verified_views.len()
            || !qglake_has_full_sha256_hashes(&bootstrap.view_version_receipt_hashes))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay evidence is missing full SHA-256 view version receipt hashes"
                .to_string(),
        ));
    }
    if !verification.verified_views.is_empty() {
        let accepted_view_receipt_hashes = verification
            .verified_view_receipt_hashes
            .values()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let replayed_view_receipt_hashes = bootstrap
            .view_version_receipt_hashes
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if accepted_view_receipt_hashes != replayed_view_receipt_hashes {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain replay evidence view version receipt hashes do not match the accepted QueryGraph bundle".to_string(),
            ));
        }
    }
    if standards_set(&bootstrap.standards) != standards_set(&verification.standards) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay standards do not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if bootstrap.policy_binding_count != expected_policy_binding_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain replay policy binding count does not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    if bootstrap.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain bootstrap replay emitted no lineage projection".to_string(),
        ));
    }
    if !qglake_has_full_sha256_hashes(&bootstrap.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&bootstrap.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain querygraph.bootstrap replayEventHashes and replayOpenLineageHashes must contain full SHA-256 sink receipt hashes"
                .to_string(),
        ));
    }
    verify_qglake_view_replay(drain, verification)?;
    verify_qglake_credential_replay(drain, principal)?;
    verify_qglake_management_list_replay(drain, expected_policy_binding_count)?;
    verify_qglake_catalog_config_replay(drain)?;
    verify_qglake_credential_replay_matches_storage_profile_upsert(drain, principal)?;
    verify_qglake_table_commit_history_replay(drain, principal)?;
    verify_qglake_scan_replay(drain)?;
    require_qglake_lineage_event_types_cover_summaries(drain)?;
    require_qglake_lineage_authorization_actions_match_events(drain)?;
    require_qglake_lineage_drain_counts_match_summaries(drain)?;
    require_qglake_lineage_drain_sink_hashes_duplicate_free(drain)?;
    Ok(())
}

pub(crate) fn verify_qglake_catalog_config_replay(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    for event in drain
        .events
        .iter()
        .filter(|event| event.event_type == "catalog.config-read")
    {
        verify_qglake_catalog_config_defaults(event)?;
        verify_qglake_catalog_config_overrides(event)?;
        verify_qglake_catalog_config_endpoints(event)?;
    }
    Ok(())
}

pub(crate) fn verify_qglake_catalog_config_defaults(
    event: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    if event.catalog_config_defaults.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain catalog config replay is missing config defaults".to_string(),
        ));
    }
    let mut keys = BTreeSet::new();
    for entry in &event.catalog_config_defaults {
        if entry.key.trim().is_empty() || entry.value.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay defaults must contain non-empty string key/value entries".to_string(),
            ));
        }
        if !keys.insert(entry.key.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay defaults must not contain duplicate keys"
                    .to_string(),
            ));
        }
    }
    let required = [
        (LAKECAT_COMPATIBILITY_KEY, LAKECAT_COMPATIBILITY_VALUE),
        (LAKECAT_FORMAT_BASELINE_KEY, LAKECAT_FORMAT_BASELINE_VALUE),
        (LAKECAT_FORMAT_V4_KEY, LAKECAT_FORMAT_V4_VALUE),
        (LAKECAT_FORMAT_V4_BRIDGE_KEY, LAKECAT_FORMAT_V4_BRIDGE_VALUE),
        (
            LAKECAT_FORMAT_V4_TYPED_SAIL_KEY,
            LAKECAT_FORMAT_V4_TYPED_SAIL_VALUE,
        ),
    ];
    let allowed_v4_keys = required
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| key.starts_with("lakecat.format.v4"))
        .collect::<BTreeSet<_>>();
    for key in &keys {
        if key.starts_with("lakecat.format.v4") && !allowed_v4_keys.contains(key) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay defaults contain unsupported v4 bridge keys"
                    .to_string(),
            ));
        }
    }
    for (required_key, required_value) in required {
        if !event
            .catalog_config_defaults
            .iter()
            .any(|entry| entry.key == required_key && entry.value == required_value)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain catalog config replay defaults must include {required_key}={required_value}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_catalog_config_overrides(
    event: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    let mut keys = BTreeSet::new();
    for entry in &event.catalog_config_overrides {
        if entry.key.trim().is_empty() || entry.value.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay overrides must contain non-empty string key/value entries".to_string(),
            ));
        }
        if !keys.insert(entry.key.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay overrides must not contain duplicate keys"
                    .to_string(),
            ));
        }
        if entry.key.starts_with("lakecat.format.v4") {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay overrides must not contain v4 bridge keys"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_catalog_config_endpoints(
    event: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    if event.catalog_config_endpoints.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain catalog config replay is missing advertised endpoints"
                .to_string(),
        ));
    }
    let mut endpoints = BTreeSet::new();
    for endpoint in &event.catalog_config_endpoints {
        if endpoint.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay endpoints must contain non-empty strings"
                    .to_string(),
            ));
        }
        if !endpoints.insert(endpoint.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain catalog config replay endpoints must not contain duplicates"
                    .to_string(),
            ));
        }
    }
    for required in required_qglake_catalog_config_endpoints() {
        if !endpoints.contains(required) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain catalog config replay endpoints must include {required}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn required_qglake_catalog_config_endpoints() -> [&'static str; 20] {
    [
        "GET /catalog/v1/config",
        "GET /catalog/v1/namespaces",
        "POST /catalog/v1/namespaces",
        "POST /catalog/v1/namespaces/{namespace}/tables",
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/commit",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "GET /catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
        "GET /catalog/v1/{warehouse}/config",
        "GET /catalog/v1/{warehouse}/namespaces",
        "POST /catalog/v1/{warehouse}/namespaces",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables",
        "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/commit",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/plan",
        "POST /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
        "GET /catalog/v1/{warehouse}/namespaces/{namespace}/tables/{table}/credentials",
        "POST /management/v1/lineage/drain",
        "GET /querygraph/v1/bootstrap",
    ]
}

pub(crate) fn require_qglake_lineage_authorization_actions_match_events(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    for event in &drain.events {
        let Some(expected_action) =
            qglake_expected_authorization_receipt_action(event.event_type.as_str())
        else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary {} has no authorization action contract",
                event.event_type
            )));
        };
        let Some(action) = event
            .authorization_receipt_action
            .as_deref()
            .filter(|action| !action.trim().is_empty())
        else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary {} is missing authorization receipt action",
                event.event_type
            )));
        };
        if action != expected_action {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary {} authorization receipt action {action} does not match expected {expected_action}",
                event.event_type
            )));
        }
    }
    Ok(())
}

pub(crate) fn qglake_expected_authorization_receipt_action(
    event_type: &str,
) -> Option<&'static str> {
    match event_type {
        "catalog.config-read" => Some("catalog-config"),
        "credentials.vend-attempted" => Some("credentials-vend"),
        "namespace.created" => Some("namespace-create"),
        "namespace.dropped" => Some("namespace-drop"),
        "namespace.listed" => Some("namespace-list"),
        "namespace.loaded" => Some("namespace-load"),
        "policy-binding.listed" | "policy-binding.upserted" => Some("policy-manage"),
        "project.listed" | "project.upserted" => Some("project-manage"),
        "querygraph.bootstrap" => Some("graph-read"),
        "server.listed" | "server.upserted" => Some("server-manage"),
        "storage-profile.listed" | "storage-profile.upserted" => Some("storage-profile-manage"),
        "table.commit" => Some("table-commit"),
        "table.commits-listed" | "table.loaded" => Some("table-load"),
        "table.created" => Some("table-create"),
        "table.deleted" => Some("table-drop"),
        "table.restored" => Some("table-restore"),
        "table.scan-planned" | "table.scan-tasks-fetched" => Some("table-plan-scan"),
        "view.dropped" => Some("view-drop"),
        "view.listed"
        | "view.loaded"
        | "view.version-receipts-listed"
        | "view.version-receipt-chains-listed" => Some("view-load"),
        "view.upserted" => Some("view-manage"),
        "warehouse.listed" | "warehouse.upserted" => Some("warehouse-manage"),
        _ => None,
    }
}

pub(crate) fn require_qglake_lineage_drain_sink_hashes_duplicate_free(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    for event in &drain.events {
        require_qglake_duplicate_free_strings(
            &event.replay_event_hashes,
            &format!(
                "qglake lineage drain {} replayEventHashes",
                event.event_type
            ),
        )?;
        require_qglake_duplicate_free_strings(
            &event.replay_open_lineage_hashes,
            &format!(
                "qglake lineage drain {} openLineageHashes",
                event.event_type
            ),
        )?;
        require_qglake_duplicate_free_strings(
            &event.view_version_receipt_hashes,
            &format!(
                "qglake lineage drain {} viewVersionReceiptHashes",
                event.event_type
            ),
        )?;
        require_qglake_duplicate_free_strings(
            &event.view_version_receipt_chain_hashes,
            &format!(
                "qglake lineage drain {} viewVersionReceiptChainHashes",
                event.event_type
            ),
        )?;
    }
    Ok(())
}

pub(crate) fn require_qglake_duplicate_free_strings(
    values: &[String],
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} must be duplicate-free"
            )));
        }
    }
    Ok(())
}

pub(crate) fn require_qglake_lineage_drain_counts_match_summaries(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    if drain.delivered != drain.event_types.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain delivered count {} does not match eventTypes count {}",
            drain.delivered,
            drain.event_types.len()
        )));
    }
    if drain.delivered != drain.events.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain delivered count {} does not match replay summary count {}",
            drain.delivered,
            drain.events.len()
        )));
    }
    let mut event_ids = BTreeSet::new();
    for (index, event) in drain.events.iter().enumerate() {
        if event.event_id.trim().is_empty() {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary event id at index {index} must be non-empty"
            )));
        }
        if !event_ids.insert(event.event_id.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary event id at index {index} must be duplicate-free"
            )));
        }
    }
    let summed_graph_events = drain
        .events
        .iter()
        .map(|event| event.graph_events)
        .sum::<usize>();
    if drain.graph_events != summed_graph_events {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain graphEvents count {} does not match replay summary graph event count {}",
            drain.graph_events, summed_graph_events
        )));
    }
    let summed_lineage_events = drain
        .events
        .iter()
        .map(|event| event.lineage_events)
        .sum::<usize>();
    if drain.lineage_events != summed_lineage_events {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain lineageEvents count {} does not match replay summary lineage event count {}",
            drain.lineage_events, summed_lineage_events
        )));
    }
    Ok(())
}

pub(crate) fn require_qglake_lineage_event_types_cover_summaries(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    let declared_event_types = drain.event_types.iter().collect::<BTreeSet<_>>();
    for summary in &drain.events {
        if !declared_event_types.contains(&summary.event_type) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain replay summary {} was not declared in event types",
                summary.event_type
            )));
        }
    }
    let declared_counts = qglake_event_type_counts(drain.event_types.iter().map(String::as_str));
    let summary_counts =
        qglake_event_type_counts(drain.events.iter().map(|event| event.event_type.as_str()));
    if declared_counts != summary_counts {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain eventTypes multiset does not match replay summary event types"
                .to_string(),
        ));
    }
    for (index, (declared, summary)) in drain.event_types.iter().zip(&drain.events).enumerate() {
        if declared != &summary.event_type {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain eventTypes order drift at index {index}: declared {declared} but replay summary is {}",
                summary.event_type
            )));
        }
    }
    Ok(())
}

pub(crate) fn qglake_event_type_counts<'a>(
    event_types: impl IntoIterator<Item = &'a str>,
) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for event_type in event_types {
        *counts.entry(event_type).or_default() += 1;
    }
    counts
}

pub(crate) fn verify_qglake_scan_replay(
    drain: &LineageDrainResponse,
) -> lakecat_core::LakeCatResult<()> {
    let planned = qglake_drain_event(drain, "table.scan-planned").ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay scan planning evidence".to_string(),
        )
    })?;
    if planned.lineage_events == 0
        || planned.graph_events == 0
        || planned
            .authorization_receipt_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || planned
            .request_identity_state
            .as_deref()
            .map_or(true, str::is_empty)
        || !qglake_has_full_sha256_hashes(&planned.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&planned.replay_open_lineage_hashes)
        || planned.scan_task_count.unwrap_or_default() == 0
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay is missing compact task, graph, or SHA-256 receipt evidence"
                .to_string(),
        ));
    }

    let fetched = qglake_drain_event(drain, "table.scan-tasks-fetched").ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay scan task fetch evidence".to_string(),
        )
    })?;
    if fetched.lineage_events == 0
        || fetched
            .authorization_receipt_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || fetched
            .request_identity_state
            .as_deref()
            .map_or(true, str::is_empty)
        || !qglake_has_full_sha256_hashes(&fetched.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&fetched.replay_open_lineage_hashes)
        || fetched.file_scan_task_count.unwrap_or_default() == 0
        || fetched.delete_file_count.unwrap_or_default() == 0
        || fetched.child_plan_task_count.unwrap_or_default() == 0
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay is missing compact file/delete task or SHA-256 receipt evidence"
                .to_string(),
        ));
    }
    verify_qglake_scan_restriction_replay(planned, fetched)?;
    Ok(())
}

pub(crate) fn verify_qglake_scan_restriction_replay(
    planned: &LineageDrainEventSummary,
    fetched: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    let planned_restriction = qglake_lineage_drain_read_restriction(planned, "scan planning")?;
    let fetched_restriction = qglake_lineage_drain_read_restriction(fetched, "scan task fetch")?;
    require_read_restriction_evidence(
        planned_restriction,
        "qglake lineage drain scan planning read restriction",
    )?;
    require_read_restriction_evidence(
        fetched_restriction,
        "qglake lineage drain scan task fetch read restriction",
    )?;
    for field in [
        "policy-hashes",
        "allowed-columns",
        "row-predicate",
        "purpose",
        "max-credential-ttl-seconds",
    ] {
        require_value_match(
            planned_restriction,
            field,
            required_value(
                fetched_restriction,
                field,
                "qglake lineage drain scan task fetch read restriction",
            )?,
            "qglake lineage drain scan planning read restriction",
        )?;
    }

    let fetched_allowed_columns = required_value(
        fetched_restriction,
        "allowed-columns",
        "qglake lineage drain scan task fetch read restriction",
    )?;
    let fetched_projection = Value::Array(
        fetched
            .required_projection
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    if &fetched_projection != fetched_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay required projection does not match fetched read restriction"
                .to_string(),
        ));
    }
    let fetched_effective_projection = Value::Array(
        fetched
            .effective_projection
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    if &fetched_effective_projection != fetched_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay effective projection does not match fetched read restriction"
                .to_string(),
        ));
    }

    let fetched_row_predicate = required_value(
        fetched_restriction,
        "row-predicate",
        "qglake lineage drain scan task fetch read restriction",
    )?;
    let expected_filters = vec![fetched_row_predicate.clone()];
    if fetched.required_filters.as_slice() != expected_filters.as_slice() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan task fetch replay required filters do not exactly preserve fetched row predicate"
                .to_string(),
        ));
    }
    if planned.requested_projection.is_empty() || planned.effective_projection.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay is missing requested/effective projection evidence"
                .to_string(),
        ));
    }
    require_non_empty_unique_strings(
        &planned.requested_projection,
        "qglake lineage drain scan planning requested projection",
    )?;
    require_non_empty_unique_strings(
        &planned.effective_projection,
        "qglake lineage drain scan planning effective projection",
    )?;
    if planned.requested_projection.len() <= planned.effective_projection.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay does not prove projection narrowing"
                .to_string(),
        ));
    }
    let requested_projection = planned
        .requested_projection
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &planned.effective_projection {
        if !requested_projection.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain scan planning replay effective projection field {field} was not requested"
            )));
        }
    }
    let effective_projection = Value::Array(
        planned
            .effective_projection
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    let planned_allowed_columns = required_value(
        planned_restriction,
        "allowed-columns",
        "qglake lineage drain scan planning read restriction",
    )?;
    if &effective_projection != planned_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay effective projection does not match planned read restriction"
                .to_string(),
        ));
    }
    if planned.requested_stats_fields.is_empty() || planned.effective_stats_fields.is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay is missing requested/effective stats-field evidence"
                .to_string(),
        ));
    }
    require_non_empty_unique_strings(
        &planned.requested_stats_fields,
        "qglake lineage drain scan planning requested stats fields",
    )?;
    require_non_empty_unique_strings(
        &planned.effective_stats_fields,
        "qglake lineage drain scan planning effective stats fields",
    )?;
    if planned.requested_stats_fields.len() <= planned.effective_stats_fields.len() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay does not prove stats-field narrowing"
                .to_string(),
        ));
    }
    let requested_stats = planned
        .requested_stats_fields
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &planned.effective_stats_fields {
        if !requested_stats.contains(field.as_str()) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain scan planning replay effective stats field {field} was not requested"
            )));
        }
    }
    let effective_stats = Value::Array(
        planned
            .effective_stats_fields
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    );
    let planned_allowed_columns = required_value(
        planned_restriction,
        "allowed-columns",
        "qglake lineage drain scan planning read restriction",
    )?;
    if &effective_stats != planned_allowed_columns {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain scan planning replay effective stats fields do not match planned read restriction"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn qglake_lineage_drain_read_restriction<'a>(
    event: &'a LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<&'a serde_json::Map<String, Value>> {
    event
        .read_restriction
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(|| {
            lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain {label} replay is missing governed read restriction evidence"
            ))
        })
}

pub(crate) fn verify_qglake_replay_artifacts(
    bundle: &QueryGraphBootstrap,
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<QueryGraphBootstrapVerification> {
    let verification = bundle.verify_manifest()?;
    verify_qglake_querygraph_import_contract(bundle)?;
    verify_qglake_lineage_drain(
        drain,
        &verification,
        principal,
        qglake_policy_binding_count(bundle),
    )?;
    Ok(verification)
}

pub(crate) fn verify_qglake_view_replay(
    drain: &LineageDrainResponse,
    verification: &QueryGraphBootstrapVerification,
) -> lakecat_core::LakeCatResult<()> {
    for view_stable_id in &verification.verified_views {
        let Some(view_replay) = drain.events.iter().find(|event| {
            matches!(
                event.event_type.as_str(),
                "view.upserted" | "view.loaded" | "view.dropped"
            ) && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
        }) else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain did not replay view evidence for {view_stable_id}"
            )));
        };
        if view_replay.graph_events == 0 || view_replay.lineage_events == 0 {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain view replay for {view_stable_id} did not emit graph and lineage projections"
            )));
        }
        if let Some(expected_version) = verification.verified_view_versions.get(view_stable_id) {
            if view_replay.view_version != Some(*expected_version) {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view replay for {view_stable_id} did not preserve accepted view version {expected_version}"
                )));
            }
        }
        if view_replay
            .view_warehouse
            .as_deref()
            .map_or(true, str::is_empty)
            || view_replay.view_namespace.is_empty()
            || view_replay.view_name.as_deref().map_or(true, str::is_empty)
            || !qglake_has_full_sha256_hashes(&view_replay.replay_event_hashes)
            || !qglake_has_full_sha256_hashes(&view_replay.replay_open_lineage_hashes)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain view replay for {view_stable_id} is missing compact identity or full SHA-256 receipt hashes"
            )));
        }
        if drain.events.iter().any(|event| {
            event.event_type == "view.dropped"
                && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
        }) {
            let drop_replay = drain.events.iter().find(|event| {
                event.event_type == "view.dropped"
                    && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
            });
            if let (Some(drop_replay), Some(expected_version)) = (
                drop_replay,
                verification.verified_view_versions.get(view_stable_id),
            ) {
                if drop_replay.expected_view_version != Some(*expected_version) {
                    return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                        "qglake lineage drain view drop replay for {view_stable_id} did not preserve expected view version {expected_version}"
                    )));
                }
            }
            let Some(tombstone_receipts) = drain.events.iter().find(|event| {
                event.event_type == "view.version-receipts-listed"
                    && event.view_stable_id.as_deref() == Some(view_stable_id.as_str())
                    && qglake_has_full_sha256_hashes(&event.view_version_receipt_hashes)
            }) else {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view drop replay for {view_stable_id} is missing full SHA-256 tombstone receipt evidence"
                )));
            };
            if tombstone_receipts.lineage_events == 0 {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain tombstone receipt replay for {view_stable_id} emitted no lineage projection"
                )));
            }
            let Some(receipt_chain_read) = drain.events.iter().find(|event| {
                event.event_type == "view.version-receipt-chains-listed"
                    && event.view_warehouse == view_replay.view_warehouse
                    && event.view_namespace == view_replay.view_namespace
                    && qglake_has_full_sha256_hashes(&event.view_version_receipt_chain_hashes)
                    && event.view_version_receipt_chain_verified_count > 0
                    && qglake_has_full_sha256_hashes(&event.view_version_receipt_hashes)
            }) else {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view drop replay for {view_stable_id} is missing full SHA-256 namespace receipt-chain evidence for the accepted view namespace"
                )));
            };
            if receipt_chain_read.view_version_receipt_chain_hashes.len()
                != receipt_chain_read.view_version_receipt_chain_verified_count
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain namespace receipt-chain replay for {view_stable_id} verified-chain count does not match chain hash evidence"
                )));
            }
            if receipt_chain_read.view_version_receipt_hashes.len()
                < receipt_chain_read.view_version_receipt_chain_hashes.len()
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain namespace receipt-chain replay for {view_stable_id} receipt hashes do not cover verified chain hashes"
                )));
            }
            let tombstone_hashes = tombstone_receipts
                .view_version_receipt_hashes
                .iter()
                .collect::<BTreeSet<_>>();
            let receipt_chain_hashes = receipt_chain_read
                .view_version_receipt_hashes
                .iter()
                .collect::<BTreeSet<_>>();
            if !tombstone_hashes.is_subset(&receipt_chain_hashes) {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain view drop replay for {view_stable_id} tombstone receipt hashes are not covered by namespace receipt-chain evidence"
                )));
            }
            if receipt_chain_read.lineage_events == 0
                || !qglake_has_full_sha256_hashes(&receipt_chain_read.replay_event_hashes)
                || !qglake_has_full_sha256_hashes(&receipt_chain_read.replay_open_lineage_hashes)
            {
                return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                    "qglake lineage drain namespace receipt-chain replay for {view_stable_id} is missing chain, lineage, or full SHA-256 sink receipt hashes"
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_credential_replay(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    let expected_restricted_subject = principal.unwrap_or("anonymous");
    let expected_restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let Some(restricted_probe) =
        qglake_credential_event(drain, expected_restricted_subject, expected_restricted_kind)
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the restricted credential probe".to_string(),
        ));
    };
    if restricted_probe.credential_count != Some(0)
        || restricted_probe.raw_credential_exception_allowed != Some(false)
        || restricted_probe.credential_block_reason.as_deref()
            != Some(QGLAKE_RESTRICTED_CREDENTIAL_BLOCK_REASON)
        || restricted_probe.raw_credential_exception_reason.is_some()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain restricted credential replay did not prove raw credentials were blocked"
                .to_string(),
        ));
    }
    verify_qglake_credential_lineage_projection(restricted_probe, "restricted")?;

    let Some(human_probe) = qglake_credential_event(drain, "human:qglake-operator", "human") else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the trusted human credential probe".to_string(),
        ));
    };
    if human_probe.credential_count.unwrap_or_default() == 0
        || human_probe.raw_credential_exception_allowed != Some(true)
        || human_probe.credential_block_reason.is_some()
        || human_probe.raw_credential_exception_reason.as_deref()
            != Some(QGLAKE_HUMAN_RAW_CREDENTIAL_EXCEPTION_REASON)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain trusted human credential replay did not prove audited standard credential vending"
                .to_string(),
        ));
    }
    verify_qglake_credential_lineage_projection(human_probe, "trusted human")?;
    let restricted_ttl = qglake_event_max_credential_ttl_seconds(restricted_probe).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain restricted credential replay is missing max credential TTL evidence"
                .to_string(),
        )
    })?;
    let human_ttl = qglake_event_max_credential_ttl_seconds(human_probe).ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain trusted human credential replay is missing max credential TTL evidence"
                .to_string(),
        )
    })?;
    if human_ttl != restricted_ttl {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain trusted human credential replay TTL cap mismatch: expected={restricted_ttl} actual={human_ttl}"
        )));
    }
    verify_qglake_credential_restriction_match(restricted_probe, human_probe)?;
    Ok(())
}

pub(crate) fn verify_qglake_credential_lineage_projection(
    event: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_credential_prefix_hashes(event, label)?;
    if event.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay emitted no lineage projection"
        )));
    }
    if event.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay emitted no credential-root graph projection"
        )));
    }
    if qglake_event_max_credential_ttl_seconds(event).is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing max credential TTL evidence"
        )));
    }
    let Some(authorization_receipt_hash) = event
        .authorization_receipt_hash
        .as_deref()
        .filter(|hash| is_full_sha256_hash(hash))
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing full SHA-256 authorization receipt hash evidence"
        )));
    };
    if authorization_receipt_hash.trim().is_empty() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay authorization receipt hash must be non-empty"
        )));
    }
    if event.authorization_receipt_action.as_deref() != Some("credentials-vend") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay authorization receipt action must be credentials-vend"
        )));
    }
    let restriction = qglake_lineage_drain_read_restriction(event, &format!("{label} credential"))?;
    require_read_restriction_evidence(
        restriction,
        &format!("qglake lineage drain {label} credential read restriction"),
    )?;
    verify_qglake_credential_storage_profile_projection(event, label)?;
    if !qglake_has_full_sha256_hashes(&event.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&event.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing full SHA-256 sink receipt hashes"
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_credential_prefix_hashes(
    event: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    let credential_count = event.credential_count.ok_or_else(|| {
        lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing credential count evidence"
        ))
    })?;
    if event.credential_prefix_hashes.len() != credential_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay credentialPrefixHashes count mismatch: expected={credential_count} actual={}",
            event.credential_prefix_hashes.len()
        )));
    }
    if credential_count == 0 {
        return Ok(());
    }
    if !qglake_has_full_sha256_hashes(&event.credential_prefix_hashes) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing full SHA-256 credential prefix hash evidence"
        )));
    }
    require_qglake_duplicate_free_strings(
        &event.credential_prefix_hashes,
        &format!("qglake lineage drain {label} credential credentialPrefixHashes"),
    )
}

pub(crate) fn verify_qglake_credential_restriction_match(
    restricted: &LineageDrainEventSummary,
    human: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    let restricted_restriction =
        qglake_lineage_drain_read_restriction(restricted, "restricted credential")?;
    let human_restriction =
        qglake_lineage_drain_read_restriction(human, "trusted human credential")?;
    for field in [
        "policy-hashes",
        "allowed-columns",
        "row-predicate",
        "purpose",
        "max-credential-ttl-seconds",
    ] {
        require_value_match(
            restricted_restriction,
            field,
            required_value(
                human_restriction,
                field,
                "qglake lineage drain trusted human credential read restriction",
            )?,
            "qglake lineage drain restricted credential read restriction",
        )?;
    }
    Ok(())
}

pub(crate) fn verify_qglake_credential_storage_profile_projection(
    event: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if event
        .storage_profile_id
        .as_deref()
        .map_or(true, str::is_empty)
        || event
            .storage_profile_provider
            .as_deref()
            .map_or(true, str::is_empty)
        || event
            .storage_profile_issuance_mode
            .as_deref()
            .map_or(true, str::is_empty)
        || event
            .storage_profile_location_prefix_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || event.storage_profile_secret_ref_present.is_none()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing redacted storage-profile graph evidence"
        )));
    }
    verify_qglake_storage_profile_provider_issuance_mode(
        event
            .storage_profile_provider
            .as_deref()
            .unwrap_or_default(),
        event
            .storage_profile_issuance_mode
            .as_deref()
            .unwrap_or_default(),
        &format!("qglake lineage drain {label} credential replay storage-profile"),
    )?;
    if event.storage_profile_secret_ref_present == Some(false)
        && event.storage_profile_secret_ref_provider.is_some()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay carried a secret-ref provider without secret-ref presence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(false)
        && event.storage_profile_secret_ref_hash.is_some()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay carried a secret-ref hash without secret-ref presence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_provider
            .as_deref()
            .map_or(true, |provider| provider.trim().is_empty())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing secret-ref provider evidence"
        )));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay is missing full SHA-256 secret-ref hash evidence"
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_credential_replay_matches_storage_profile_upsert(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    let Some(storage_profile_upsert) = drain
        .events
        .iter()
        .find(|event| event.event_type == "storage-profile.upserted")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay storage profile upsert evidence".to_string(),
        ));
    };
    let expected_restricted_subject = principal.unwrap_or("anonymous");
    let expected_restricted_kind = if principal.is_some() {
        "agent"
    } else {
        "anonymous"
    };
    let Some(restricted_probe) =
        qglake_credential_event(drain, expected_restricted_subject, expected_restricted_kind)
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the restricted credential probe".to_string(),
        ));
    };
    verify_qglake_credential_storage_profile_matches_upsert(
        restricted_probe,
        storage_profile_upsert,
        "restricted",
    )?;

    let Some(human_probe) = qglake_credential_event(drain, "human:qglake-operator", "human") else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay the trusted human credential probe".to_string(),
        ));
    };
    verify_qglake_credential_storage_profile_matches_upsert(
        human_probe,
        storage_profile_upsert,
        "trusted human",
    )
}

pub(crate) fn verify_qglake_credential_storage_profile_matches_upsert(
    credential: &LineageDrainEventSummary,
    storage_profile_upsert: &LineageDrainEventSummary,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if credential.storage_profile_id != storage_profile_upsert.storage_profile_id
        || credential.storage_profile_provider != storage_profile_upsert.storage_profile_provider
        || credential.storage_profile_issuance_mode
            != storage_profile_upsert.storage_profile_issuance_mode
        || credential.storage_profile_location_prefix_hash
            != storage_profile_upsert.storage_profile_location_prefix_hash
        || credential.storage_profile_secret_ref_present
            != storage_profile_upsert.storage_profile_secret_ref_present
        || credential.storage_profile_secret_ref_provider
            != storage_profile_upsert.storage_profile_secret_ref_provider
        || credential.storage_profile_secret_ref_hash
            != storage_profile_upsert.storage_profile_secret_ref_hash
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} credential replay storage-profile evidence does not match storage profile upsert replay"
        )));
    }
    Ok(())
}

pub(crate) fn verify_qglake_management_list_replay(
    drain: &LineageDrainResponse,
    expected_policy_binding_count: usize,
) -> lakecat_core::LakeCatResult<()> {
    let Some(policy_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "policy-binding.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay policy list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(policy_list, "policy list", true)?;
    if policy_list.policy_binding_count != expected_policy_binding_count {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy list replay count does not match the accepted QueryGraph bundle"
                .to_string(),
        ));
    }
    verify_qglake_management_ids(
        &policy_list.policy_ids,
        policy_list.policy_binding_count,
        "policy list",
    )?;
    let Some(policy_upsert) = drain
        .events
        .iter()
        .find(|event| event.event_type == "policy-binding.upserted")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay policy binding upsert evidence".to_string(),
        ));
    };
    verify_qglake_policy_binding_upsert_replay(policy_upsert, &policy_list.policy_ids)?;
    let Some(storage_profile_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "storage-profile.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay storage profile list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(storage_profile_list, "storage profile list", true)?;
    if storage_profile_list
        .storage_profile_count
        .unwrap_or_default()
        == 0
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile list replay did not expose any storage profiles"
                .to_string(),
        ));
    }
    verify_qglake_management_ids(
        &storage_profile_list.storage_profile_ids,
        storage_profile_list
            .storage_profile_count
            .unwrap_or_default(),
        "storage profile list",
    )?;
    let Some(storage_profile_upsert) = drain
        .events
        .iter()
        .find(|event| event.event_type == "storage-profile.upserted")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay storage profile upsert evidence".to_string(),
        ));
    };
    verify_qglake_storage_profile_upsert_replay(storage_profile_upsert)?;
    let Some(server_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "server.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay server list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(server_list, "server list", false)?;
    if server_list.server_count.unwrap_or_default() == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain server list replay did not expose any servers".to_string(),
        ));
    }
    verify_qglake_management_ids(
        &server_list.server_ids,
        server_list.server_count.unwrap_or_default(),
        "server list",
    )?;
    let Some(project_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "project.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay project list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(project_list, "project list", false)?;
    if project_list.project_count.unwrap_or_default() == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain project list replay did not expose any projects".to_string(),
        ));
    }
    verify_qglake_management_ids(
        &project_list.project_ids,
        project_list.project_count.unwrap_or_default(),
        "project list",
    )?;
    let Some(warehouse_list) = drain
        .events
        .iter()
        .find(|event| event.event_type == "warehouse.listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay warehouse list evidence".to_string(),
        ));
    };
    verify_qglake_management_list_receipts(warehouse_list, "warehouse list", false)?;
    if warehouse_list.warehouse_count.unwrap_or_default() == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain warehouse list replay did not expose any warehouses".to_string(),
        ));
    }
    verify_qglake_management_ids(
        &warehouse_list.warehouse_names,
        warehouse_list.warehouse_count.unwrap_or_default(),
        "warehouse list",
    )?;
    if let Some(project_id) = warehouse_list.management_scope_project_id.as_deref() {
        if !is_qglake_compact_management_id(project_id) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain warehouse list replay contains syntactically invalid project scope evidence"
                    .to_string(),
            ));
        }
        if !project_list
            .project_ids
            .iter()
            .any(|listed_project_id| listed_project_id == project_id)
        {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain warehouse list project scope does not match project list evidence"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn verify_qglake_policy_binding_upsert_replay(
    event: &LineageDrainEventSummary,
    policy_ids: &[String],
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_management_list_receipts(event, "policy binding upsert", true)?;
    let Some(policy_id) = event
        .policy_id
        .as_deref()
        .filter(|policy_id| !policy_id.trim().is_empty())
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay is missing policy id evidence"
                .to_string(),
        ));
    };
    if !is_qglake_compact_management_id(policy_id) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay contains syntactically invalid policy id evidence"
                .to_string(),
        ));
    }
    if !policy_ids
        .iter()
        .any(|listed_policy_id| listed_policy_id == policy_id)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert policy id does not match policy list evidence"
                .to_string(),
        ));
    }
    if event
        .policy_odrl_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay is missing ODRL hash evidence"
                .to_string(),
        ));
    }
    if event
        .principal_subject
        .as_deref()
        .map_or(true, str::is_empty)
        || event.principal_kind.as_deref().map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay is missing principal evidence"
                .to_string(),
        ));
    }
    if event
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay is missing authorization receipt hash evidence"
                .to_string(),
        ));
    }
    if event.authorization_receipt_action.as_deref() != Some("policy-manage") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay authorization receipt action must be policy-manage"
                .to_string(),
        ));
    }
    if event.graph_events == 0 || event.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain policy binding upsert replay emitted no graph or lineage projection"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn verify_qglake_management_ids(
    ids: &[String],
    expected_count: usize,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if ids.len() != expected_count || ids.iter().any(|id| id.trim().is_empty()) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing compact management ID evidence"
        )));
    }
    let mut seen = BTreeSet::new();
    if ids.iter().any(|id| !seen.insert(id)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay contains duplicate compact management ID evidence"
        )));
    }
    if ids.iter().any(|id| !is_qglake_compact_management_id(id)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay contains syntactically invalid compact management ID evidence"
        )));
    }
    Ok(())
}

pub(crate) fn is_qglake_compact_management_id(id: &str) -> bool {
    if id.trim() != id || id.is_empty() {
        return false;
    }
    if id == "." || id == ".." {
        return false;
    }
    if id
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace() || matches!(ch, '/' | '\\' | '?' | '#'))
    {
        return false;
    }
    true
}

pub(crate) fn verify_qglake_table_commit_history_replay(
    drain: &LineageDrainResponse,
    principal: Option<&str>,
) -> lakecat_core::LakeCatResult<()> {
    let Some(commit_history) = drain
        .events
        .iter()
        .find(|event| event.event_type == "table.commits-listed")
    else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain did not replay table commit history evidence".to_string(),
        ));
    };
    if commit_history.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay emitted no lineage projection"
                .to_string(),
        ));
    }
    if commit_history.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay emitted no graph projection"
                .to_string(),
        ));
    }
    let Some(commit_principal_kind) = commit_history.principal_kind.as_deref() else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing principal kind evidence"
                .to_string(),
        ));
    };
    if commit_principal_kind != "agent" {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain table commit history replay principal kind did not match accepted principal kind agent: actual={commit_principal_kind}"
        )));
    }
    if let Some(expected_principal) = principal {
        let Some(commit_principal) = commit_history.principal_subject.as_deref() else {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain table commit history replay is missing principal subject evidence"
                    .to_string(),
            ));
        };
        if commit_principal != expected_principal {
            return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "qglake lineage drain table commit history replay principal did not match accepted principal {expected_principal}: actual={commit_principal}"
            )));
        }
    }
    if !qglake_has_full_sha256_hashes(&commit_history.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&commit_history.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing full SHA-256 receipt hashes"
                .to_string(),
        ));
    }
    if !commit_history
        .authorization_receipt_hash
        .as_deref()
        .is_some_and(|hash| is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing authorization receipt hash evidence"
                .to_string(),
        ));
    }
    if commit_history.authorization_receipt_action.as_deref() != Some("table-load") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay table.commits-listed authorization receipt action does not match expected table-load"
                .to_string(),
        ));
    }
    let Some(commit_count) = commit_history.table_commit_count else {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing compact commit summary or SHA-256 commit hash evidence"
                .to_string(),
        ));
    };
    if commit_count > 0
        && (commit_history.table_commit_sequence_numbers.is_empty()
            || !qglake_has_full_sha256_hashes(&commit_history.table_commit_hashes))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay is missing compact commit summary or SHA-256 commit hash evidence"
                .to_string(),
        ));
    }
    if commit_history.table_commit_sequence_numbers.len() != commit_count
        || commit_history.table_commit_hashes.len() != commit_count
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain table commit history replay count does not match sequence-number and commit-hash evidence"
                .to_string(),
        ));
    }
    let mut unique_commit_hashes = BTreeSet::new();
    for commit_hash in &commit_history.table_commit_hashes {
        if !unique_commit_hashes.insert(commit_hash) {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain table commit history replay commit hashes must not contain duplicates"
                    .to_string(),
            ));
        }
    }
    let mut previous_sequence = 0;
    for sequence_number in &commit_history.table_commit_sequence_numbers {
        if *sequence_number == 0 || *sequence_number <= previous_sequence {
            return Err(lakecat_core::LakeCatError::InvalidArgument(
                "qglake lineage drain table commit history replay sequence numbers must be positive and strictly increasing"
                    .to_string(),
            ));
        }
        previous_sequence = *sequence_number;
    }
    Ok(())
}

pub(crate) fn verify_qglake_storage_profile_upsert_replay(
    event: &LineageDrainEventSummary,
) -> lakecat_core::LakeCatResult<()> {
    verify_qglake_management_list_receipts(event, "storage profile upsert", true)?;
    if event
        .storage_profile_id
        .as_deref()
        .unwrap_or_default()
        .is_empty()
        || event
            .storage_profile_provider
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        || event
            .storage_profile_issuance_mode
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        || event
            .storage_profile_location_prefix_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
        || event.storage_profile_secret_ref_present.is_none()
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay did not expose redacted credential-root evidence"
                .to_string(),
        ));
    }
    verify_qglake_storage_profile_provider_issuance_mode(
        event
            .storage_profile_provider
            .as_deref()
            .unwrap_or_default(),
        event
            .storage_profile_issuance_mode
            .as_deref()
            .unwrap_or_default(),
        "qglake lineage drain storage profile upsert replay",
    )?;
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_provider
            .as_deref()
            .map_or(true, |provider| provider.trim().is_empty())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing secret-ref provider evidence"
                .to_string(),
        ));
    }
    if event.storage_profile_secret_ref_present == Some(true)
        && event
            .storage_profile_secret_ref_hash
            .as_deref()
            .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing full SHA-256 secret-ref hash evidence"
                .to_string(),
        ));
    }
    if event.storage_profile_secret_ref_present == Some(false)
        && (event.storage_profile_secret_ref_provider.is_some()
            || event.storage_profile_secret_ref_hash.is_some())
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay carried secret-ref evidence without secret-ref presence"
                .to_string(),
        ));
    }
    if event
        .principal_subject
        .as_deref()
        .map_or(true, str::is_empty)
        || event.principal_kind.as_deref().map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing principal evidence"
                .to_string(),
        ));
    }
    if event
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay is missing authorization receipt hash evidence"
                .to_string(),
        ));
    }
    if event.authorization_receipt_action.as_deref() != Some("storage-profile-manage") {
        return Err(lakecat_core::LakeCatError::InvalidArgument(
            "qglake lineage drain storage profile upsert replay authorization receipt action must be storage-profile-manage"
                .to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn verify_qglake_storage_profile_provider_issuance_mode(
    provider: &str,
    issuance_mode: &str,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    match issuance_mode {
        "local-file-no-secret" if provider != "file" => {
            Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} local-file-no-secret issuance mode requires file provider"
            )))
        }
        "short-lived-secret-ref" if !matches!(provider, "s3" | "gcs" | "azure") => {
            Err(lakecat_core::LakeCatError::InvalidArgument(format!(
                "{label} short-lived-secret-ref issuance mode requires s3, gcs, or azure provider"
            )))
        }
        "local-file-no-secret" | "short-lived-secret-ref" => Ok(()),
        _ => Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} issuance mode must be local-file-no-secret or short-lived-secret-ref"
        ))),
    }
}

pub(crate) fn verify_qglake_management_list_receipts(
    event: &LineageDrainEventSummary,
    label: &str,
    require_warehouse_scope: bool,
) -> lakecat_core::LakeCatResult<()> {
    if event.lineage_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay emitted no lineage projection"
        )));
    }
    if event.graph_events == 0 {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay emitted no catalog graph projection"
        )));
    }
    if require_warehouse_scope
        && event
            .management_scope_warehouse
            .as_deref()
            .map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing compact management scope"
        )));
    }
    if event
        .principal_subject
        .as_deref()
        .map_or(true, str::is_empty)
        || event.principal_kind.as_deref().map_or(true, str::is_empty)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing principal evidence"
        )));
    }
    if event
        .authorization_receipt_hash
        .as_deref()
        .map_or(true, |hash| !is_full_sha256_hash(hash))
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing full SHA-256 authorization receipt hash evidence"
        )));
    }
    if !qglake_has_full_sha256_hashes(&event.replay_event_hashes)
        || !qglake_has_full_sha256_hashes(&event.replay_open_lineage_hashes)
    {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "qglake lineage drain {label} replay is missing full SHA-256 receipt hashes"
        )));
    }
    Ok(())
}

pub(crate) fn qglake_has_full_sha256_hashes(hashes: &[String]) -> bool {
    !hashes.is_empty() && hashes.iter().all(|hash| is_full_sha256_hash(hash))
}

pub(crate) fn verify_qglake_typedid_hash_pair(
    envelope_hash: Option<&str>,
    proof_hash: Option<&str>,
    label: &str,
) -> lakecat_core::LakeCatResult<()> {
    if envelope_hash.is_some_and(|hash| !is_full_sha256_hash(hash)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} TypeDID envelope hash must be full SHA-256-shaped"
        )));
    }
    if proof_hash.is_some_and(|hash| !is_full_sha256_hash(hash)) {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} TypeDID proof hash must be full SHA-256-shaped"
        )));
    }
    if proof_hash.is_some() && envelope_hash.is_none() {
        return Err(lakecat_core::LakeCatError::InvalidArgument(format!(
            "{label} TypeDID proof hash requires an envelope hash"
        )));
    }
    Ok(())
}

pub(crate) fn standards_set(standards: &[String]) -> BTreeSet<&str> {
    standards.iter().map(String::as_str).collect()
}

pub(crate) fn qglake_policy_binding_count(bundle: &QueryGraphBootstrap) -> usize {
    bundle
        .tables
        .iter()
        .map(|table| table.policy_bindings.len())
        .sum()
}

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
