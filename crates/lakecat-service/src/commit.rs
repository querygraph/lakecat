use axum::Json;
use axum::http::HeaderMap;
use lakecat_api::{CommitTableRequest, CommitTableResponse};
use lakecat_core::sail::CommitPreparationRequest;
use lakecat_core::{LakeCatError, WarehouseName, content_hash_json};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_store::{TableCommit, table_ident};
use serde_json::json;

use crate::*;

pub(crate) async fn commit_table_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
    request: CommitTableRequest,
) -> Result<Json<CommitTableResponse>, LakeCatHttpError> {
    let idempotency_key = request_idempotency_key(&headers)?;
    let identity = request_identity(&headers)?;
    let ident = table_ident(warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_commit(&state, identity, ident).await?;
    let idempotency_request_hash = idempotency_key
        .as_ref()
        .map(|_| {
            content_hash_json(&json!({
                "requirements": &request.requirements,
                "updates": &request.updates,
                "metadata-location": &request.metadata_location,
                "metadata": &request.metadata,
            }))
        })
        .transpose()?;
    if let (Some(idempotency_key), Some(idempotency_request_hash)) =
        (&idempotency_key, &idempotency_request_hash)
        && let Some(table) = state
            .store
            .replay_table_commit(
                capability.table(),
                idempotency_key,
                idempotency_request_hash,
            )
            .await?
    {
        return Ok(Json(CommitTableResponse {
            metadata_location: table.metadata_location,
            metadata: table.metadata,
        }));
    }
    let current = state.store.load_table(capability.table()).await?;
    let storage_profile = state.store.storage_profile_for_table(&current).await?;
    let current_metadata_location = current.metadata_location.clone();
    let commit_plan = state
        .sail
        .prepare_commit(CommitPreparationRequest {
            table: capability.table().clone(),
            principal: capability.receipt().principal.clone(),
            current_metadata_location: current_metadata_location.clone(),
            new_metadata_location: request.metadata_location,
            current_metadata: current.metadata,
            new_metadata: request.metadata,
            requirements: request.requirements,
            updates: request.updates,
        })
        .await?;
    validate_planned_metadata_location(
        &commit_plan,
        current_metadata_location.as_deref(),
        &storage_profile,
    )?;
    let metadata_write = write_planned_metadata(&commit_plan).await?;
    let table = match state
        .store
        .commit_table(
            capability.table(),
            TableCommit {
                requirements: commit_plan.requirements,
                updates: commit_plan.updates,
                expected_previous_metadata_location: current_metadata_location.clone(),
                new_metadata_location: commit_plan.new_metadata_location.clone(),
                new_metadata: Some(commit_plan.new_metadata.clone()),
                idempotency_key,
                idempotency_request_hash,
                principal: capability.receipt().principal.clone(),
                authorization_receipt: Some(serde_json::to_value(capability.receipt()).map_err(
                    |err| {
                        LakeCatError::Internal(format!(
                            "failed to encode authorization receipt: {err}"
                        ))
                    },
                )?),
            },
        )
        .await
    {
        Ok(table) => table,
        Err(err) => {
            let err = cleanup_planned_metadata_after_commit_error(
                metadata_write,
                current_metadata_location.as_deref(),
                err,
            )
            .await;
            return Err(err.into());
        }
    };
    Ok(Json(CommitTableResponse {
        metadata_location: table.metadata_location,
        metadata: table.metadata,
    }))
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedMetadataWrite {
    pub(crate) location: String,
}
