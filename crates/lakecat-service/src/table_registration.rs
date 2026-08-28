use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use lakecat_api::{LoadTableResponse, RegisterTableRequest};
use lakecat_core::{LakeCatError, TableName, WarehouseName};
use lakecat_store::{CatalogAuditEvent, TableRecord};
use serde_json::{Value, json};

use crate::*;

pub(crate) async fn register_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(namespace): Path<String>,
    Json(request): Json<RegisterTableRequest>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    register_table_in_warehouse(state.warehouse.clone(), state, headers, namespace, request).await
}

pub(crate) async fn register_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace)): Path<(String, String)>,
    Json(request): Json<RegisterTableRequest>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    register_table_in_warehouse(warehouse, state, headers, namespace, request).await
}

pub(crate) async fn register_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    request: RegisterTableRequest,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let ident = rest_table_ident(
        warehouse.as_str(),
        namespace,
        TableName::new(request.name)?.as_str(),
    )?;
    let capability = authorize_table_register(&state, request_identity(&headers)?, ident).await?;
    if request.overwrite {
        return Err(LakeCatError::InvalidArgument(
            "register table does not permit overwrite=true".to_string(),
        )
        .into());
    }
    state
        .store
        .load_namespace(&capability.table().warehouse, &capability.table().namespace)
        .await?;
    let metadata_location = request.metadata_location;
    let metadata = read_metadata_object(&metadata_location).await?;
    let location = registered_table_location(&metadata)?;
    let table = TableRecord::new(
        capability.table().clone(),
        location,
        Some(metadata_location.clone()),
        metadata,
        capability.receipt().principal.clone(),
    );
    table.validate()?;
    let storage_profile = state.store.storage_profile_for_table(&table).await?;
    validate_metadata_object_location(&metadata_location, None, &storage_profile)?;
    let table = state.store.create_table(table).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.registered",
            Some(table.ident.clone()),
            capability.receipt().principal.clone(),
            json!({
                "event-type": "table.registered",
                "table": table.ident,
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

fn registered_table_location(metadata: &Value) -> Result<String, LakeCatError> {
    metadata
        .get("location")
        .and_then(Value::as_str)
        .filter(|location| !location.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            LakeCatError::InvalidArgument(
                "registered table metadata must contain a non-empty location".to_string(),
            )
        })
}
