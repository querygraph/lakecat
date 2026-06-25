use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use lakecat_api::{
    CatalogConfigResponse, CommitTableRequest, CommitTableResponse, ConfigEntry,
    CreateNamespaceRequest, CreateTableRequest, ListNamespacesResponse, ListPolicyBindingsResponse,
    ListProjectsResponse, ListServersResponse, ListStorageProfilesResponse,
    ListTableCommitRecordsResponse, ListViewVersionReceiptChainsResponse,
    ListViewVersionReceiptsResponse, ListViewsResponse, ListWarehousesResponse,
    LoadCredentialsResponse, LoadTableResponse, NamespaceResponse, PolicyBindingResponse,
    ProjectResponse, ServerResponse, StorageCredential, StorageProfileResponse,
    UpsertPolicyBindingRequest, UpsertProjectRequest, UpsertServerRequest,
    UpsertStorageProfileRequest, UpsertViewRequest, UpsertWarehouseRequest, ViewResponse,
    WarehouseResponse,
};
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, TableIdent, TableName, WarehouseName,
    content_hash_bytes, content_hash_json,
};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_security::{AuthorizationReceipt, ReadRestriction, ViewDropCapability};
use lakecat_store::{
    CatalogAuditEvent, CredentialIssuanceMode, PolicyBinding, ProjectRecord, ServerRecord,
    StorageProfile, StorageProvider, TableRecord, ViewRecord, WarehouseRecord, table_ident,
};
use serde_json::{Value, json};

use crate::*;

pub(crate) async fn get_config(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<CatalogConfigResponse>, LakeCatHttpError> {
    get_config_in_warehouse(state.warehouse.clone(), state, headers).await
}

pub(crate) async fn get_config_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
) -> Result<Json<CatalogConfigResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    get_config_in_warehouse(warehouse, state, headers).await
}

pub(crate) async fn get_config_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
) -> Result<Json<CatalogConfigResponse>, LakeCatHttpError> {
    let capability = authorize_catalog_config(&state, request_identity(&headers)?).await?;
    let config = CatalogConfigResponse::default();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "catalog.config-read",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "catalog.config-read",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
                "defaults": &config.defaults,
                "overrides": &config.overrides,
                "endpoints": &config.endpoints,
            }),
        )?)
        .await?;
    Ok(Json(config))
}

pub(crate) async fn create_namespace(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Json(request): Json<CreateNamespaceRequest>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    create_namespace_in_warehouse(state.warehouse.clone(), state, headers, request).await
}

pub(crate) async fn create_namespace_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
    Json(request): Json<CreateNamespaceRequest>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    create_namespace_in_warehouse(warehouse, state, headers, request).await
}

pub(crate) async fn create_namespace_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    request: CreateNamespaceRequest,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let capability = authorize_namespace_create(&state, identity).await?;
    let namespace = Namespace::new(request.namespace)?;
    state
        .store
        .create_namespace(&warehouse, namespace.clone())
        .await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "namespace.created",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "namespace.created",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
            }),
        )?)
        .await?;
    Ok(Json(NamespaceResponse::from_namespace(&namespace)))
}

pub(crate) async fn list_namespaces(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListNamespacesResponse>, LakeCatHttpError> {
    list_namespaces_in_warehouse(state.warehouse.clone(), state, headers).await
}

pub(crate) async fn list_namespaces_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
) -> Result<Json<ListNamespacesResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    list_namespaces_in_warehouse(warehouse, state, headers).await
}

pub(crate) async fn list_namespaces_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
) -> Result<Json<ListNamespacesResponse>, LakeCatHttpError> {
    let capability = authorize_namespace_list(&state, request_identity(&headers)?).await?;
    let namespaces = state.store.list_namespaces(&warehouse).await?;
    let namespace_paths: Vec<String> = namespaces.iter().map(Namespace::path).collect();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "namespace.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "namespace.listed",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
                "namespace-count": namespaces.len(),
                "namespace-paths": namespace_paths,
            }),
        )?)
        .await?;
    Ok(Json(ListNamespacesResponse {
        namespaces: namespaces
            .into_iter()
            .map(|namespace| namespace.parts().to_vec())
            .collect(),
    }))
}

pub(crate) async fn load_namespace(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(namespace): Path<String>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    load_namespace_in_warehouse(state.warehouse.clone(), state, headers, namespace).await
}

pub(crate) async fn load_namespace_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    load_namespace_in_warehouse(warehouse, state, headers, namespace).await
}

pub(crate) async fn load_namespace_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let capability = authorize_namespace_load(&state, request_identity(&headers)?).await?;
    let namespace = namespace.parse::<Namespace>()?;
    let namespace = state.store.load_namespace(&warehouse, &namespace).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "namespace.loaded",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "namespace.loaded",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
            }),
        )?)
        .await?;
    Ok(Json(NamespaceResponse::from_namespace(&namespace)))
}

pub(crate) async fn drop_namespace(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(namespace): Path<String>,
) -> Result<StatusCode, LakeCatHttpError> {
    drop_namespace_in_warehouse(state.warehouse.clone(), state, headers, namespace).await
}

pub(crate) async fn drop_namespace_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    drop_namespace_in_warehouse(warehouse, state, headers, namespace).await
}

pub(crate) async fn drop_namespace_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
) -> Result<StatusCode, LakeCatHttpError> {
    let capability = authorize_namespace_drop(&state, request_identity(&headers)?).await?;
    let namespace = namespace.parse::<Namespace>()?;
    let namespace = state.store.drop_namespace(&warehouse, &namespace).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "namespace.dropped",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "namespace.dropped",
                "authorization-receipt": capability.receipt(),
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
            }),
        )?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn create_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(namespace): Path<String>,
    Json(request): Json<CreateTableRequest>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    create_table_in_warehouse(state.warehouse.clone(), state, headers, namespace, request).await
}

pub(crate) async fn create_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
    Json(request): Json<CreateTableRequest>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    create_table_in_warehouse(warehouse, state, headers, namespace, request).await
}

pub(crate) async fn create_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    request: CreateTableRequest,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(
        warehouse.as_str(),
        namespace,
        TableName::new(request.name)?.as_str(),
    )?;
    let capability = authorize_table_create(&state, identity, ident).await?;
    let principal = capability.receipt().principal.clone();
    let ident = capability.table().clone();

    // Two creation paths:
    //  - standard Iceberg REST `createTable`: client sends a `schema`, the
    //    catalog derives a location and generates the initial metadata;
    //  - register-style: client supplies its own `metadata` (+ location).
    let (location, metadata_location, metadata) = if request.metadata.is_object() {
        // Register-style path: keep the existing behavior, location required.
        let location = request.location.clone().ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "create table requires a location when metadata is supplied".to_string(),
            )
        })?;
        (location, request.metadata_location, request.metadata)
    } else if let Some(schema) = request.schema.as_ref() {
        // Standard path: synthesize initial metadata from the schema.
        let location = request.location.clone().unwrap_or_else(|| {
            format!(
                "file:///tmp/lakecat/{}/{}/{}",
                ident.warehouse.as_str(),
                ident.namespace,
                ident.name.as_str()
            )
        });
        let table_uuid = uuid::Uuid::new_v4().to_string();
        let metadata = lakecat_core::sail::initial_table_metadata(
            &table_uuid,
            &location,
            schema,
            request.partition_spec.as_ref(),
            request.write_order.as_ref(),
            request
                .properties
                .as_ref()
                .unwrap_or(&serde_json::Value::Null),
        );
        let metadata_location = Some(format!(
            "{location}/metadata/00000-{table_uuid}.metadata.json"
        ));
        (location, metadata_location, metadata)
    } else {
        return Err(LakeCatError::InvalidArgument(
            "create table requires either a schema (standard createTable) or a metadata document"
                .to_string(),
        )
        .into());
    };

    let table = TableRecord::new(
        ident.clone(),
        location,
        metadata_location,
        metadata,
        principal.clone(),
    );
    let table = state.store.create_table(table).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.created",
            Some(ident.clone()),
            principal.clone(),
            json!({
                "event-type": "table.created",
                "table": ident,
                "authorization-receipt": capability.receipt(),
                "metadata-location": table.metadata_location,
                "location": table.location,
                "format-version": table_metadata_format_version(&table.metadata),
                "metadata-graph": table_metadata_graph_summary(&table.metadata),
                "version": table.version,
            }),
        )?)
        .await?;
    Ok(Json(load_table_response(table)))
}

pub(crate) async fn load_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    load_table_in_warehouse(state.warehouse.clone(), state, headers, namespace, table).await
}

pub(crate) async fn load_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    load_table_in_warehouse(warehouse, state, headers, namespace, table).await
}

pub(crate) async fn load_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_load(&state, identity, ident).await?;
    let table = state.store.load_table(capability.table()).await?;
    let ident = capability.table().clone();
    let principal = capability.receipt().principal.clone();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.loaded",
            Some(ident.clone()),
            principal.clone(),
            json!({
                "event-type": "table.loaded",
                "table": ident,
                "authorization-receipt": capability.receipt(),
                "metadata-location": table.metadata_location,
                "format-version": table_metadata_format_version(&table.metadata),
                "metadata-graph": table_metadata_graph_summary(&table.metadata),
                "version": table.version,
            }),
        )?)
        .await?;
    Ok(Json(load_table_response(table)))
}

pub(crate) async fn delete_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<StatusCode, LakeCatHttpError> {
    delete_table_in_warehouse(state.warehouse.clone(), state, headers, namespace, table).await
}

pub(crate) async fn delete_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    delete_table_in_warehouse(warehouse, state, headers, namespace, table).await
}

pub(crate) async fn delete_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
) -> Result<StatusCode, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_drop(&state, identity, ident).await?;
    let ident = capability.table().clone();
    state
        .store
        .soft_delete_table(
            &ident,
            capability.receipt().principal.clone(),
            Some(serde_json::to_value(capability.receipt()).map_err(|err| {
                LakeCatError::Internal(format!("failed to encode drop receipt: {err}"))
            })?),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn restore_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_restore(&state, identity, ident).await?;
    let restored = state
        .store
        .restore_table(
            capability.table(),
            capability.receipt().principal.clone(),
            Some(serde_json::to_value(capability.receipt()).map_err(|err| {
                LakeCatError::Internal(format!("failed to encode restore receipt: {err}"))
            })?),
        )
        .await?;
    Ok(Json(load_table_response(restored)))
}

pub(crate) async fn list_table_commits(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<Json<ListTableCommitRecordsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability =
        authorize_table_load(&state, request_identity(&headers)?, ident.clone()).await?;
    state.store.load_table(capability.table()).await?;
    let records = state
        .store
        .table_commit_records(capability.table(), 0, None)
        .await?;
    let commits = records
        .iter()
        .map(table_commit_record_response)
        .collect::<LakeCatResult<Vec<_>>>()?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.commits-listed",
            Some(capability.table().clone()),
            capability.receipt().principal.clone(),
            json!({
                "event-type": "table.commits-listed",
                "warehouse": capability.table().warehouse.as_str(),
                "namespace": capability.table().namespace.parts(),
                "table": capability.table().name.as_str(),
                "commit-count": commits.len(),
                "commit-hashes": commits
                    .iter()
                    .map(|commit| commit.commit_hash.clone())
                    .collect::<Vec<_>>(),
                "sequence-numbers": commits
                    .iter()
                    .map(|commit| commit.sequence_number)
                    .collect::<Vec<_>>(),
                "principal-subject": capability.receipt().principal.subject,
                "principal-kind": principal_kind_name(&capability.receipt().principal.kind),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListTableCommitRecordsResponse { commits }))
}

pub(crate) async fn load_credentials(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<Json<LoadCredentialsResponse>, LakeCatHttpError> {
    load_credentials_in_warehouse(state.warehouse.clone(), state, headers, namespace, table).await
}

pub(crate) async fn load_credentials_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
) -> Result<Json<LoadCredentialsResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    load_credentials_in_warehouse(warehouse, state, headers, namespace, table).await
}

pub(crate) async fn load_credentials_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
) -> Result<Json<LoadCredentialsResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_credentials_vend(&state, identity, ident).await?;
    let table = state.store.load_table(capability.table()).await?;
    let storage_profile = state.store.storage_profile_for_table(&table).await?;
    let read_restriction = capability.read_restriction()?;
    let max_credential_ttl_seconds = read_restriction.max_credential_ttl_seconds;
    let raw_exception = capability
        .receipt()
        .context
        .get("lakecat:raw-credential-exception");
    let credential_block_reason = if let Some(exception) = raw_exception {
        (exception.get("allowed").and_then(Value::as_bool) == Some(false)).then(|| {
            exception
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("fine-grained read restriction requires Sail-planned reads")
        })
    } else {
        read_restriction
            .requires_governed_read()
            .then_some("fine-grained read restriction requires Sail-planned reads")
    };
    let storage_credentials = if credential_block_reason.is_some() {
        Vec::new()
    } else {
        canonicalize_credential_response_evidence(
            issued_credentials_for_profile(
                state
                    .credential_issuer
                    .issue(CredentialIssuanceRequest {
                        table: table.clone(),
                        profile: storage_profile.clone(),
                        authorization_receipt: capability.receipt().clone(),
                        max_credential_ttl_seconds,
                    })
                    .await?,
                &storage_profile,
                max_credential_ttl_seconds,
            )?,
            &storage_profile,
            capability.receipt(),
            read_restriction.requires_governed_read(),
            max_credential_ttl_seconds,
        )
    };
    let ident = capability.table().clone();
    let mut audit_payload = credentials_vend_audit_payload(
        &ident,
        &table,
        &storage_profile,
        &storage_credentials,
        capability.receipt(),
    )?;
    if let Some(reason) = credential_block_reason {
        audit_payload["lakecat:credential-block-reason"] = json!(reason);
    }
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "credentials.vend-attempted",
            Some(ident.clone()),
            capability.receipt().principal.clone(),
            audit_payload,
        )?)
        .await?;
    Ok(Json(LoadCredentialsResponse {
        storage_credentials,
    }))
}

pub(crate) fn credentials_vend_audit_payload(
    ident: &TableIdent,
    table: &TableRecord,
    storage_profile: &StorageProfile,
    storage_credentials: &[StorageCredential],
    receipt: &AuthorizationReceipt,
) -> LakeCatResult<Value> {
    let mut audit_payload = json!({
        "event-type": "credentials.vend-attempted",
        "table": ident.clone(),
        "authorization-receipt": receipt,
        "storage-location": table.location,
        "storage-profile-id": storage_profile.profile_id,
        "storage-profile": {
            "profile-id": storage_profile.profile_id,
            "warehouse": storage_profile.warehouse.as_str(),
            "provider": storage_profile.provider.as_str(),
            "issuance-mode": storage_profile.issuance_mode.as_str(),
            "secret-ref-present": storage_profile.secret_ref.is_some(),
            "location-prefix-hash": content_hash_json(&json!({
                "location-prefix": storage_profile.location_prefix
            }))?,
        },
        "secret-ref-present": storage_profile.secret_ref.is_some(),
        "credential-count": storage_credentials.len(),
        "credential-response-evidence": credential_response_evidence(
            storage_credentials,
            storage_profile,
            receipt
        )?,
        "mode": storage_profile.issuance_mode.as_str(),
    });
    if let Some(provider) = storage_profile
        .secret_ref
        .as_deref()
        .and_then(secret_ref_provider_label)
    {
        audit_payload["storage-profile"]["secret-ref-provider"] = json!(provider);
    }
    if let Some(secret_ref) = storage_profile.secret_ref.as_deref() {
        audit_payload["storage-profile"]["secret-ref-hash"] =
            json!(content_hash_bytes(secret_ref.as_bytes()));
    }
    if let Some(restriction) = receipt.context.get("read-restriction") {
        audit_payload["read-restriction"] = restriction.clone();
    }
    if let Some(exception) = receipt.context.get("lakecat:raw-credential-exception") {
        audit_payload["lakecat:raw-credential-exception"] = exception.clone();
    }
    Ok(audit_payload)
}

pub(crate) fn credential_response_evidence(
    storage_credentials: &[StorageCredential],
    storage_profile: &StorageProfile,
    receipt: &AuthorizationReceipt,
) -> LakeCatResult<Value> {
    Ok(Value::Array(
        storage_credentials
            .iter()
            .map(|credential| {
                let non_lakecat_config = credential
                    .config
                    .iter()
                    .filter(|entry| !entry.key.to_ascii_lowercase().starts_with("lakecat."))
                    .cloned()
                    .collect::<Vec<_>>();
                let non_lakecat_config =
                    serde_json::to_value(non_lakecat_config).map_err(|err| {
                        LakeCatError::Internal(format!(
                            "failed to serialize credential response evidence: {err}"
                        ))
                    })?;
                Ok(json!({
                    "prefix-hash": content_hash_json(&json!({
                        "credential-prefix": &credential.prefix
                    }))?,
                    "storage-profile-id": single_config_value(
                        &credential.config,
                        "lakecat.storage-profile-id"
                    ),
                    "storage-provider": single_config_value(
                        &credential.config,
                        "lakecat.storage-provider"
                    ),
                    "credential-mode": single_config_value(
                        &credential.config,
                        "lakecat.credential-mode"
                    ),
                    "authorization-principal": single_config_value(
                        &credential.config,
                        "lakecat.authorization-principal"
                    ),
                    "governed-read-required": single_config_value(
                        &credential.config,
                        "lakecat.governed-read-required"
                    ),
                    "max-credential-ttl-seconds": single_config_value(
                        &credential.config,
                        "lakecat.max-credential-ttl-seconds"
                    ),
                    "secret-ref-provider": single_config_value(
                        &credential.config,
                        "lakecat.secret-ref-provider"
                    ),
                    "secret-ref-hash": single_config_value(
                        &credential.config,
                        "lakecat.secret-ref-hash"
                    ),
                    "issuer-config-entry-count": non_lakecat_config
                        .as_array()
                        .map_or(0, Vec::len),
                    "issuer-config-hash": content_hash_json(&non_lakecat_config)?,
                    "catalog-profile-id": &storage_profile.profile_id,
                    "receipt-principal": &receipt.principal.subject,
                }))
            })
            .collect::<LakeCatResult<Vec<_>>>()?,
    ))
}

pub(crate) fn single_config_value(config: &[ConfigEntry], key: &str) -> Option<String> {
    let mut values = config
        .iter()
        .filter(|entry| entry.key == key)
        .map(|entry| entry.value.clone());
    let first = values.next()?;
    values.next().is_none().then_some(first)
}

pub(crate) fn table_scan_planned_audit_payload(
    ident: &TableIdent,
    table: &TableRecord,
    receipt: &AuthorizationReceipt,
    scan: &lakecat_core::sail::ScanPlan,
    scan_request_extensions: &Value,
) -> Value {
    let mut audit_payload = json!({
        "event-type": "table.scan-planned",
        "table": ident,
        "authorization-receipt": receipt,
        "planned-by": scan.planned_by,
        "snapshot-id": scan.snapshot_id,
        "scan-task-count": scan.scan_tasks.len(),
        "storage-location": table.location,
        "metadata-location": table.metadata_location,
    });
    if let Some(restriction) = receipt.context.get("read-restriction") {
        audit_payload["read-restriction"] = restriction.clone();
        append_read_restriction_requirements(&mut audit_payload, restriction);
    }
    for field in [
        "requested-projection",
        "effective-projection",
        "requested-stats-fields",
        "effective-stats-fields",
    ] {
        if let Some(value) = scan_request_extensions.get(field) {
            audit_payload[field] = value.clone();
        }
    }
    audit_payload
}

pub(crate) fn table_scan_tasks_fetched_audit_payload(
    ident: &TableIdent,
    table: &TableRecord,
    receipt: &AuthorizationReceipt,
    fetched: &lakecat_core::sail::FetchScanTasksPlan,
    fetch_extensions: &Value,
) -> Value {
    let mut audit_payload = json!({
        "event-type": "table.scan-tasks-fetched",
        "table": ident,
        "authorization-receipt": receipt,
        "planned-by": fetched.planned_by,
        "snapshot-id": fetched.snapshot_id,
        "plan-task": fetched.plan_task,
        "file-scan-task-count": fetched.file_scan_tasks.len(),
        "delete-file-count": fetched.delete_files.len(),
        "child-plan-task-count": fetched.plan_tasks.len(),
        "storage-location": table.location,
        "metadata-location": table.metadata_location,
    });
    if let Some(restriction) = receipt.context.get("read-restriction") {
        audit_payload["read-restriction"] = restriction.clone();
        append_read_restriction_requirements(&mut audit_payload, restriction);
    }
    for field in [
        "requested-stats-fields",
        "effective-stats-fields",
        "stats-fields",
    ] {
        if let Some(value) = fetch_extensions.get(field) {
            audit_payload[field] = value.clone();
        }
    }
    audit_payload
}

pub(crate) fn append_read_restriction_requirements(audit_payload: &mut Value, restriction: &Value) {
    if let Ok(restriction) = serde_json::from_value::<ReadRestriction>(restriction.clone()) {
        if let Ok(required_projection) = restriction.effective_projection(&[]) {
            audit_payload["required-projection"] = json!(required_projection.clone());
            audit_payload["effective-projection"] = json!(required_projection);
            audit_payload["required-filters"] = json!(restriction.mandatory_filters());
        }
    }
}

pub(crate) fn table_metadata_graph_summary(metadata: &Value) -> Value {
    json!({
        "current-schema-id": metadata.get("current-schema-id").cloned().unwrap_or(Value::Null),
        "fields": metadata_current_schema_fields(metadata),
        "current-snapshot-id": metadata.get("current-snapshot-id").cloned().unwrap_or(Value::Null),
        "current-snapshot": metadata_current_snapshot(metadata).cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn table_metadata_format_version(metadata: &Value) -> Option<i64> {
    metadata.get("format-version").and_then(Value::as_i64)
}

pub(crate) fn metadata_current_schema_fields(metadata: &Value) -> Vec<Value> {
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
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn metadata_current_snapshot(metadata: &Value) -> Option<&Value> {
    let current_snapshot_id = metadata
        .get("current-snapshot-id")
        .and_then(Value::as_i64)?;
    metadata
        .get("snapshots")
        .and_then(Value::as_array)
        .and_then(|snapshots| {
            snapshots.iter().find(|snapshot| {
                snapshot.get("snapshot-id").and_then(Value::as_i64) == Some(current_snapshot_id)
            })
        })
}

pub(crate) async fn list_storage_profiles(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
) -> Result<Json<ListStorageProfilesResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_storage_profile_manage(&state, request_identity(&headers)?).await?;
    let profiles = state.store.list_storage_profiles(&warehouse).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "storage-profile.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "storage-profile.listed",
                "warehouse": warehouse.as_str(),
                "authorization-receipt": capability.receipt(),
                "storage-profile-count": profiles.len(),
                "storage-profile-ids": profiles
                    .iter()
                    .map(|profile| profile.profile_id.as_str())
                    .collect::<Vec<_>>(),
            }),
        )?)
        .await?;
    Ok(Json(ListStorageProfilesResponse {
        storage_profiles: profiles.iter().map(storage_profile_response).collect(),
    }))
}

pub(crate) async fn list_warehouses(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListWarehousesResponse>, LakeCatHttpError> {
    let capability = authorize_warehouse_manage(&state, request_identity(&headers)?).await?;
    let warehouses = state.store.list_warehouses().await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "warehouse.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "warehouse.listed",
                "warehouse-count": warehouses.len(),
                "warehouse-names": warehouses
                    .iter()
                    .map(|warehouse| warehouse.warehouse.as_str())
                    .collect::<Vec<_>>(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListWarehousesResponse {
        warehouses: warehouses.iter().map(warehouse_response).collect(),
    }))
}

pub(crate) async fn list_project_warehouses(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Result<Json<ListWarehousesResponse>, LakeCatHttpError> {
    let capability = authorize_warehouse_manage(&state, request_identity(&headers)?).await?;
    let warehouses = state.store.list_project_warehouses(&project).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "warehouse.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "warehouse.listed",
                "project-id": project.as_str(),
                "warehouse-count": warehouses.len(),
                "warehouse-names": warehouses
                    .iter()
                    .map(|warehouse| warehouse.warehouse.as_str())
                    .collect::<Vec<_>>(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListWarehousesResponse {
        warehouses: warehouses.iter().map(warehouse_response).collect(),
    }))
}

pub(crate) async fn list_projects(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListProjectsResponse>, LakeCatHttpError> {
    let capability = authorize_project_manage(&state, request_identity(&headers)?).await?;
    let projects = state.store.list_projects().await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "project.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "project.listed",
                "project-count": projects.len(),
                "project-ids": projects
                    .iter()
                    .map(|project| project.project_id.as_str())
                    .collect::<Vec<_>>(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListProjectsResponse {
        projects: projects.iter().map(project_response).collect(),
    }))
}

pub(crate) async fn list_servers(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListServersResponse>, LakeCatHttpError> {
    let capability = authorize_server_manage(&state, request_identity(&headers)?).await?;
    let servers = state.store.list_servers().await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "server.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "server.listed",
                "server-count": servers.len(),
                "server-ids": servers
                    .iter()
                    .map(|server| server.server_id.as_str())
                    .collect::<Vec<_>>(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListServersResponse {
        servers: servers.iter().map(server_response).collect(),
    }))
}

pub(crate) async fn upsert_server(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(server): Path<String>,
    Json(request): Json<UpsertServerRequest>,
) -> Result<Json<ServerResponse>, LakeCatHttpError> {
    let capability = authorize_server_manage(&state, request_identity(&headers)?).await?;
    let record = ServerRecord::new(
        server,
        request.display_name,
        request.endpoint_url,
        request.properties,
        capability.receipt().principal.clone(),
    )?;
    let record = state.store.upsert_server(record).await?;
    let event_payload = redact_server_event_payload(json!({
        "event-type": "server.upserted",
        "server-id": record.server_id.as_str(),
        "server-record": server_response(&record),
        "authorization-receipt": capability.receipt(),
    }));
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "server.upserted",
            None,
            capability.receipt().principal.clone(),
            event_payload,
        )?)
        .await?;
    Ok(Json(server_response(&record)))
}

pub(crate) async fn upsert_project(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Json(request): Json<UpsertProjectRequest>,
) -> Result<Json<ProjectResponse>, LakeCatHttpError> {
    let capability = authorize_project_manage(&state, request_identity(&headers)?).await?;
    let record = ProjectRecord::new(
        project,
        request.server_id,
        request.display_name,
        request.properties,
        capability.receipt().principal.clone(),
    )?;
    let record = state.store.upsert_project(record).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "project.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "project.upserted",
                "project-id": record.project_id.as_str(),
                "project-record": project_response(&record),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(project_response(&record)))
}

pub(crate) async fn upsert_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
    Json(request): Json<UpsertWarehouseRequest>,
) -> Result<Json<WarehouseResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_warehouse_manage(&state, request_identity(&headers)?).await?;
    let record = WarehouseRecord::new(
        warehouse.clone(),
        request.project_id.unwrap_or_else(|| "default".to_string()),
        request.storage_root,
        request.properties,
        capability.receipt().principal.clone(),
    )?;
    let record = state.store.upsert_warehouse(record).await?;
    let event_payload = redact_warehouse_event_payload(json!({
        "event-type": "warehouse.upserted",
        "warehouse": warehouse.as_str(),
        "warehouse-record": warehouse_response(&record),
        "authorization-receipt": capability.receipt(),
    }));
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "warehouse.upserted",
            None,
            capability.receipt().principal.clone(),
            event_payload,
        )?)
        .await?;
    Ok(Json(warehouse_response(&record)))
}

pub(crate) async fn upsert_project_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((project, warehouse)): Path<(String, String)>,
    Json(request): Json<UpsertWarehouseRequest>,
) -> Result<Json<WarehouseResponse>, LakeCatHttpError> {
    if let Some(request_project) = request.project_id.as_deref()
        && request_project != project
    {
        return Err(LakeCatError::InvalidArgument(format!(
            "warehouse project id {request_project} does not match route project {project}"
        ))
        .into());
    }
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_warehouse_manage(&state, request_identity(&headers)?).await?;
    let record = WarehouseRecord::new(
        warehouse.clone(),
        project,
        request.storage_root,
        request.properties,
        capability.receipt().principal.clone(),
    )?;
    let record = state.store.upsert_warehouse(record).await?;
    let event_payload = redact_warehouse_event_payload(json!({
        "event-type": "warehouse.upserted",
        "project-id": record.project_id.as_str(),
        "warehouse": warehouse.as_str(),
        "warehouse-record": warehouse_response(&record),
        "authorization-receipt": capability.receipt(),
    }));
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "warehouse.upserted",
            None,
            capability.receipt().principal.clone(),
            event_payload,
        )?)
        .await?;
    Ok(Json(warehouse_response(&record)))
}

pub(crate) async fn upsert_storage_profile(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, profile)): Path<(String, String)>,
    Json(request): Json<UpsertStorageProfileRequest>,
) -> Result<Json<StorageProfileResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_storage_profile_manage(&state, request_identity(&headers)?).await?;
    let storage_profile = StorageProfile::new(
        profile,
        warehouse.clone(),
        request.location_prefix,
        request.provider.parse::<StorageProvider>()?,
        request.issuance_mode.parse::<CredentialIssuanceMode>()?,
        request.secret_ref,
        request.public_config,
    )?;
    let storage_profile = state.store.upsert_storage_profile(storage_profile).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "storage-profile.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "storage-profile.upserted",
                "warehouse": warehouse.as_str(),
                "storage-profile": storage_profile_event_payload(&storage_profile),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(storage_profile_response(&storage_profile)))
}

pub(crate) async fn list_views(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<Json<ListViewsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_manage(&state, request_identity(&headers)?).await?;
    let views = state.store.list_views(&warehouse, &namespace).await?;
    let view_names: Vec<String> = views
        .iter()
        .map(|view| view.name.as_str().to_string())
        .collect();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.listed",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view-count": views.len(),
                "view-names": view_names,
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListViewsResponse {
        views: views.iter().map(view_response).collect(),
    }))
}

pub(crate) async fn catalog_list_views(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<Json<ListViewsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_load(&state, request_identity(&headers)?).await?;
    let views = state.store.list_views(&warehouse, &namespace).await?;
    let view_names: Vec<String> = views
        .iter()
        .map(|view| view.name.as_str().to_string())
        .collect();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.listed",
                "interface": "iceberg-rest",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view-count": views.len(),
                "view-names": view_names,
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListViewsResponse {
        views: views.iter().map(view_response).collect(),
    }))
}

pub(crate) async fn list_view_version_receipts(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
) -> Result<Json<ListViewVersionReceiptsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_load(&state, request_identity(&headers)?).await?;
    let receipts = state
        .store
        .list_view_version_receipts(&warehouse, &namespace, &view_name)
        .await?;
    let response_receipts = receipts
        .iter()
        .map(view_version_receipt_response)
        .collect::<LakeCatResult<Vec<_>>>()?;
    let drop_receipt_hashes = response_receipts
        .iter()
        .filter(|receipt| receipt.operation == "drop")
        .map(|receipt| receipt.receipt_hash.clone())
        .collect::<Vec<_>>();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.version-receipts-listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.version-receipts-listed",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view": view_name.as_str(),
                "receipt-count": response_receipts.len(),
                "receipt-hashes": response_receipts
                    .iter()
                    .map(|receipt| receipt.receipt_hash.clone())
                    .collect::<Vec<_>>(),
                "drop-receipt-hashes": drop_receipt_hashes,
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListViewVersionReceiptsResponse {
        receipts: response_receipts,
    }))
}

pub(crate) async fn list_view_version_receipt_chains(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
) -> Result<Json<ListViewVersionReceiptChainsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_load(&state, request_identity(&headers)?).await?;
    let receipts = state
        .store
        .list_namespace_view_version_receipts(&warehouse, &namespace)
        .await?;
    let chains = view_version_receipt_chains(&receipts)?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.version-receipt-chains-listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.version-receipt-chains-listed",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "chain-count": chains.len(),
                "receipt-count": chains.iter().map(|chain| chain.receipt_count).sum::<usize>(),
                "tombstone-count": chains.iter().filter(|chain| chain.tombstoned).count(),
                "chain-verified-count": chains.iter().filter(|chain| chain.chain_verified).count(),
                "view-version-receipt-chains": chains,
                "chain-hashes": chains
                    .iter()
                    .map(|chain| chain.chain_hash.clone())
                    .collect::<Vec<_>>(),
                "receipt-hashes": chains
                    .iter()
                    .flat_map(|chain| chain.receipts.iter().map(|receipt| receipt.receipt_hash.clone()))
                    .collect::<Vec<_>>(),
                "drop-receipt-hashes": chains
                    .iter()
                    .flat_map(|chain| {
                        chain
                            .receipts
                            .iter()
                            .filter(|receipt| receipt.operation == "drop")
                            .map(|receipt| receipt.receipt_hash.clone())
                    })
                    .collect::<Vec<_>>(),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(ListViewVersionReceiptChainsResponse { chains }))
}

pub(crate) async fn catalog_load_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
) -> Result<Json<ViewResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_load(&state, request_identity(&headers)?).await?;
    let record = state
        .store
        .load_view(&warehouse, &namespace, &view_name)
        .await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.loaded",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.loaded",
                "interface": "iceberg-rest",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view": view_response(&record),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(view_response(&record)))
}

pub(crate) async fn upsert_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
    Json(request): Json<UpsertViewRequest>,
) -> Result<Json<ViewResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_manage(&state, request_identity(&headers)?).await?;
    let record = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new(view)?,
        request.sql,
        request.dialect,
        request.schema_version,
        request.properties,
        capability.receipt().principal.clone(),
    )?
    .with_columns(view_columns_from_request(request.columns)?)?;
    let record = state
        .store
        .upsert_view_if_version(record, request.expected_view_version)
        .await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.upserted",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view": view_response(&record),
                "expected-view-version": request.expected_view_version,
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(view_response(&record)))
}

pub(crate) async fn drop_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
    Query(query): Query<ViewMutationQuery>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_drop(&state, request_identity(&headers)?).await?;
    let record = state
        .store
        .drop_view_if_version(
            &warehouse,
            &namespace,
            &view_name,
            capability.receipt().principal.clone(),
            query.expected_view_version,
        )
        .await?;
    record_view_drop_audit(
        &state,
        &warehouse,
        &namespace,
        &record,
        &capability,
        None,
        query.expected_view_version,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn catalog_upsert_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
    Json(request): Json<UpsertViewRequest>,
) -> Result<Json<ViewResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let capability = authorize_view_manage(&state, request_identity(&headers)?).await?;
    let record = ViewRecord::new(
        warehouse.clone(),
        namespace.clone(),
        TableName::new(view)?,
        request.sql,
        request.dialect,
        request.schema_version,
        request.properties,
        capability.receipt().principal.clone(),
    )?
    .with_columns(view_columns_from_request(request.columns)?)?;
    let record = state
        .store
        .upsert_view_if_version(record, request.expected_view_version)
        .await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "view.upserted",
                "interface": "iceberg-rest",
                "warehouse": warehouse.as_str(),
                "namespace": namespace.parts(),
                "view": view_response(&record),
                "expected-view-version": request.expected_view_version,
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(view_response(&record)))
}

pub(crate) async fn catalog_drop_view(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, view)): Path<(String, String, String)>,
    Query(query): Query<ViewMutationQuery>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let namespace = namespace.parse::<Namespace>()?;
    let view_name = TableName::new(view)?;
    let capability = authorize_view_drop(&state, request_identity(&headers)?).await?;
    let record = state
        .store
        .drop_view_if_version(
            &warehouse,
            &namespace,
            &view_name,
            capability.receipt().principal.clone(),
            query.expected_view_version,
        )
        .await?;
    record_view_drop_audit(
        &state,
        &warehouse,
        &namespace,
        &record,
        &capability,
        Some("iceberg-rest"),
        query.expected_view_version,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn record_view_drop_audit(
    state: &LakeCatState,
    warehouse: &WarehouseName,
    namespace: &Namespace,
    record: &ViewRecord,
    capability: &ViewDropCapability,
    interface: Option<&str>,
    expected_view_version: Option<u64>,
) -> Result<(), LakeCatHttpError> {
    let mut payload = json!({
        "event-type": "view.dropped",
        "warehouse": warehouse.as_str(),
        "namespace": namespace.parts(),
        "view": view_response(record),
        "expected-view-version": expected_view_version,
        "authorization-receipt": capability.receipt(),
    });
    if let Some(interface) = interface {
        payload["interface"] = json!(interface);
    }
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "view.dropped",
            None,
            capability.receipt().principal.clone(),
            payload,
        )?)
        .await?;
    Ok(())
}

pub(crate) async fn list_policy_bindings(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
) -> Result<Json<ListPolicyBindingsResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_policy_manage(&state, request_identity(&headers)?).await?;
    let policies = state.store.list_policy_bindings(&warehouse).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "policy-binding.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "policy-binding.listed",
                "warehouse": warehouse.as_str(),
                "authorization-receipt": capability.receipt(),
                "policy-count": policies.len(),
                "policy-ids": policies
                    .iter()
                    .map(|policy| policy.policy_id.as_str())
                    .collect::<Vec<_>>(),
            }),
        )?)
        .await?;
    Ok(Json(ListPolicyBindingsResponse {
        policies: policies.iter().map(policy_binding_response).collect(),
    }))
}

pub(crate) async fn upsert_policy_binding(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, policy)): Path<(String, String)>,
    Json(request): Json<UpsertPolicyBindingRequest>,
) -> Result<Json<PolicyBindingResponse>, LakeCatHttpError> {
    let warehouse = management_warehouse(&state, warehouse)?;
    let capability = authorize_policy_manage(&state, request_identity(&headers)?).await?;
    let namespace = request.namespace.map(Namespace::new).transpose()?;
    let table = request.table.map(TableName::new).transpose()?;
    let binding = PolicyBinding::new(
        policy,
        warehouse.clone(),
        namespace,
        table,
        request.enforced,
        request.odrl,
    )?;
    let binding = state.store.upsert_policy_binding(binding).await?;
    let policy_payload = json!({
        "policy-id": binding.policy_id.as_str(),
        "warehouse": binding.warehouse.as_str(),
        "namespace": binding
            .namespace
            .as_ref()
            .map(|namespace| namespace.parts().to_vec()),
        "table": binding
            .table
            .as_ref()
            .map(|table| table.as_str().to_string()),
        "enforced": binding.enforced,
        "odrl": binding.odrl.clone(),
        "odrl-hash": content_hash_json(&binding.odrl)?,
    });
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "policy-binding.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "policy-binding.upserted",
                "warehouse": warehouse.as_str(),
                "policy": policy_payload,
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(policy_binding_response(&binding)))
}

pub(crate) async fn commit_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<CommitTableRequest>,
) -> Result<Json<CommitTableResponse>, LakeCatHttpError> {
    commit_table_in_warehouse(
        state.warehouse.clone(),
        state,
        headers,
        namespace,
        table,
        request,
    )
    .await
}

pub(crate) async fn commit_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
    Json(request): Json<CommitTableRequest>,
) -> Result<Json<CommitTableResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    commit_table_in_warehouse(warehouse, state, headers, namespace, table, request).await
}
