use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use lakecat_api::{RenameTableRequest, TableIdentifier};
use lakecat_core::{LakeCatError, Namespace, TableIdent, TableName, WarehouseName};

use crate::*;

pub(crate) async fn rename_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Json(request): Json<RenameTableRequest>,
) -> Result<StatusCode, LakeCatHttpError> {
    rename_table_in_warehouse(state.warehouse.clone(), state, headers, request).await
}

pub(crate) async fn rename_table_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(warehouse): Path<String>,
    Json(request): Json<RenameTableRequest>,
) -> Result<StatusCode, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    rename_table_in_warehouse(warehouse, state, headers, request).await
}

pub(crate) async fn rename_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    request: RenameTableRequest,
) -> Result<StatusCode, LakeCatHttpError> {
    let source = table_identifier_in_warehouse(&warehouse, request.source)?;
    let destination = table_identifier_in_warehouse(&warehouse, request.destination)?;
    let capability =
        authorize_table_rename(&state, request_identity(&headers)?, source, destination).await?;
    let authorization_receipt = serde_json::to_value(capability.receipt()).map_err(|err| {
        LakeCatError::Internal(format!("failed to encode table rename receipt: {err}"))
    })?;
    state
        .store
        .rename_table(
            capability.source(),
            capability.destination(),
            capability.receipt().principal.clone(),
            Some(authorization_receipt),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn table_identifier_in_warehouse(
    warehouse: &WarehouseName,
    identifier: TableIdentifier,
) -> Result<TableIdent, LakeCatError> {
    Ok(TableIdent::new(
        warehouse.clone(),
        Namespace::new(identifier.namespace)?,
        TableName::new(identifier.name)?,
    ))
}
