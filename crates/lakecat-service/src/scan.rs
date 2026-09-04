use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use lakecat_api::{
    FetchScanTasksRequest as ApiFetchScanTasksRequest, FetchScanTasksResponse,
    NormalizedPlanTableScanRequest, PlanTableScanRequest, PlanTableScanResponse, TableIdentifier,
};
#[cfg(not(feature = "sail-local"))]
use lakecat_core::sail::FetchScanTasksRequest as SailFetchScanTasksRequest;
#[cfg(not(feature = "sail-local"))]
use lakecat_core::sail::ScanPlanningRequest;
use lakecat_core::{LakeCatError, LakeCatResult, WarehouseName};
use lakecat_querygraph::{
    QueryGraphBootstrap, QueryGraphTenantProjection, QueryGraphViewReceiptEvidence,
};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_security::TableScanCapability;
use lakecat_store::{CatalogAuditEvent, TableRecord, ViewRecord, ViewVersionReceipt};
use serde_json::json;

use crate::*;

pub(crate) async fn plan_table_scan(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<PlanTableScanRequest>,
) -> Result<Json<PlanTableScanResponse>, LakeCatHttpError> {
    plan_table_scan_in_warehouse(
        state.warehouse.clone(),
        state,
        headers,
        namespace,
        table,
        request,
    )
    .await
}

pub(crate) async fn plan_table_scan_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
    Json(request): Json<PlanTableScanRequest>,
) -> Result<Json<PlanTableScanResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    plan_table_scan_in_warehouse(warehouse, state, headers, namespace, table, request).await
}

pub(crate) async fn plan_table_scan_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
    request: PlanTableScanRequest,
) -> Result<Json<PlanTableScanResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = rest_table_ident(warehouse.as_str(), &namespace, table)?;
    let capability = authorize_table_scan(&state, identity, ident.clone()).await?;
    let table = state.store.load_table(capability.table()).await?;
    let (scan, scan_request_extensions, governed_scan_proof) =
        plan_scan_with_capability(&state, &capability, &table, request).await?;
    let ident = capability.table().clone();
    let principal = capability.receipt().principal.clone();
    let mut audit_payload = table_scan_planned_audit_payload(
        &ident,
        &table,
        &capability,
        &scan,
        &scan_request_extensions,
    );
    if let Some(proof) = governed_scan_proof.as_ref() {
        audit_payload["governed-scan-proof"] = serde_json::to_value(proof).map_err(|error| {
            LakeCatError::Internal(format!(
                "failed to encode governed scan proof for audit evidence: {error}"
            ))
        })?;
    }
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.scan-planned",
            Some(ident.clone()),
            principal.clone(),
            audit_payload,
        )?)
        .await?;
    Ok(Json(PlanTableScanResponse {
        table: TableIdentifier::from_ident(&ident),
        planned_by: scan.planned_by,
        status: "completed".to_string(),
        snapshot_id: scan.snapshot_id,
        plan_tasks: plan_task_tokens(&scan.scan_tasks),
        lakecat_plan_tasks: scan.scan_tasks,
        file_scan_tasks: Vec::new(),
        delete_files: Vec::new(),
        residual_filter: merge_scan_request_extensions(
            scan.residual_filter,
            scan_request_extensions,
        ),
        governed_scan_proof,
    }))
}

pub(crate) async fn plan_scan_with_capability(
    state: &LakeCatState,
    capability: &TableScanCapability,
    table: &TableRecord,
    request: PlanTableScanRequest,
) -> Result<
    (
        lakecat_core::sail::ScanPlan,
        serde_json::Value,
        Option<lakecat_core::sail::GovernedScanProof>,
    ),
    LakeCatHttpError,
> {
    let NormalizedPlanTableScanRequest {
        projection: requested_projection,
        filters,
        limit,
        snapshot_id,
        case_sensitive,
        use_snapshot_schema,
        start_snapshot_id,
        end_snapshot_id,
        stats_fields: requested_stats_fields,
    } = request.into_normalized()?;
    #[cfg(feature = "sail-local")]
    let _ = &table;
    let restriction = capability.read_restriction();
    let projection = restriction.effective_projection(&requested_projection)?;
    let stats_fields = restriction.effective_stats_fields(&requested_stats_fields);
    #[cfg(feature = "sail-local")]
    let grant_requested_projection = requested_projection.clone();
    #[cfg(not(feature = "sail-local"))]
    let grant_effective_projection = projection.clone();
    let scan_request_extensions = json!({
        "case-sensitive": case_sensitive,
        "use-snapshot-schema": use_snapshot_schema,
        "start-snapshot-id": start_snapshot_id,
        "end-snapshot-id": end_snapshot_id,
        "requested-projection": requested_projection,
        "effective-projection": projection,
        "read-restriction": restriction,
        "requested-stats-fields": requested_stats_fields,
        "effective-stats-fields": stats_fields,
        "stats-fields": stats_fields,
    });
    #[cfg(feature = "sail-local")]
    let scan = {
        let provider = LakeCatCatalogProvider::new(
            "lakecat",
            capability.table().warehouse.clone(),
            state.store.clone(),
            state.sail.clone(),
            state.governance.clone(),
            capability.receipt().principal.clone(),
        );
        provider
            .plan_authorized_table_scan(
                capability,
                ProviderScanPlanningRequest {
                    projection: requested_projection,
                    filters,
                    limit,
                    snapshot_id,
                    start_snapshot_id,
                    end_snapshot_id,
                },
            )
            .await
            .map_err(catalog_provider_error)?
    };
    #[cfg(not(feature = "sail-local"))]
    let scan = state
        .sail
        .plan_scan(ScanPlanningRequest {
            table: capability.table().clone(),
            principal: capability.receipt().principal.clone(),
            metadata_location: table.metadata_location.clone(),
            table_metadata: table.metadata.clone(),
            projection,
            filters: {
                let mut filters = filters;
                if let Some(row_predicate) = restriction.row_predicate.clone() {
                    filters.push(row_predicate);
                }
                filters
            },
            limit,
            snapshot_id,
            start_snapshot_id,
            end_snapshot_id,
        })
        .await?;
    #[cfg(not(feature = "sail-local"))]
    let grant_requested_projection = requested_projection;
    #[cfg(feature = "sail-local")]
    let grant_effective_projection = projection;
    let governed_scan_grant = crate::governed_scan::issue_governed_scan_grant(
        state.catalog_identity(),
        capability,
        table,
        &scan,
        grant_requested_projection,
        grant_effective_projection,
    )?;
    let governed_scan_proof = match governed_scan_grant {
        Some(grant) => Some(state.store.save_governed_scan_grant(grant).await?.proof),
        None => None,
    };
    Ok((scan, scan_request_extensions, governed_scan_proof))
}

#[cfg(feature = "sail-local")]
pub(crate) fn catalog_provider_error(error: impl std::fmt::Display) -> LakeCatHttpError {
    let message = error.to_string();
    if message.contains("invalid argument") {
        LakeCatError::InvalidArgument(message).into()
    } else if message.contains("not found") {
        LakeCatError::NotFound {
            object: "catalog object",
            name: message,
        }
        .into()
    } else if message.contains("conflict") {
        LakeCatError::Conflict(message).into()
    } else if message.contains("not supported") {
        LakeCatError::NotSupported(message).into()
    } else {
        LakeCatError::Internal(format!("LakeCat provider scan planning failed: {message}")).into()
    }
}

pub(crate) async fn fetch_scan_tasks(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<ApiFetchScanTasksRequest>,
) -> Result<Json<FetchScanTasksResponse>, LakeCatHttpError> {
    fetch_scan_tasks_in_warehouse(
        state.warehouse.clone(),
        state,
        headers,
        namespace,
        table,
        request,
    )
    .await
}

pub(crate) async fn fetch_scan_tasks_for_warehouse(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((warehouse, namespace, table)): Path<(String, String, String)>,
    Json(request): Json<ApiFetchScanTasksRequest>,
) -> Result<Json<FetchScanTasksResponse>, LakeCatHttpError> {
    let warehouse = prefixed_catalog_warehouse(&state, warehouse).await?;
    fetch_scan_tasks_in_warehouse(warehouse, state, headers, namespace, table, request).await
}

pub(crate) async fn fetch_scan_tasks_in_warehouse(
    warehouse: WarehouseName,
    state: LakeCatState,
    headers: HeaderMap,
    namespace: String,
    table: String,
    request: ApiFetchScanTasksRequest,
) -> Result<Json<FetchScanTasksResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = rest_table_ident(warehouse.as_str(), &namespace, table)?;
    let capability = authorize_table_scan(&state, identity, ident).await?;
    let table = state.store.load_table(capability.table()).await?;
    let fetched = fetch_scan_tasks_with_capability(&state, &capability, &table, request).await?;
    let ident = capability.table().clone();
    let fetch_extensions = fetch_scan_tasks_extensions(&capability)?;
    let audit_payload = table_scan_tasks_fetched_audit_payload(
        &ident,
        &table,
        &capability,
        &fetched,
        &fetch_extensions,
    );
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.scan-tasks-fetched",
            Some(ident.clone()),
            capability.receipt().principal.clone(),
            audit_payload,
        )?)
        .await?;
    Ok(Json(FetchScanTasksResponse {
        table: TableIdentifier::from_ident(&ident),
        planned_by: fetched.planned_by,
        plan_task: fetched.plan_task,
        snapshot_id: fetched.snapshot_id,
        file_scan_tasks: fetched.file_scan_tasks,
        delete_files: fetched.delete_files,
        plan_tasks: plan_task_tokens(&fetched.plan_tasks),
        lakecat_plan_tasks: fetched.plan_tasks,
        residual_filter: merge_fetch_scan_tasks_extensions(
            fetched.residual_filter,
            fetch_extensions,
        ),
    }))
}

pub(crate) async fn fetch_scan_tasks_with_capability(
    state: &LakeCatState,
    capability: &TableScanCapability,
    table: &TableRecord,
    request: ApiFetchScanTasksRequest,
) -> Result<lakecat_core::sail::FetchScanTasksPlan, LakeCatHttpError> {
    #[cfg(feature = "sail-local")]
    let _ = &table;
    #[cfg(not(feature = "sail-local"))]
    let restriction = capability.read_restriction();
    #[cfg(feature = "sail-local")]
    let fetched = {
        let provider = LakeCatCatalogProvider::new(
            "lakecat",
            capability.table().warehouse.clone(),
            state.store.clone(),
            state.sail.clone(),
            state.governance.clone(),
            capability.receipt().principal.clone(),
        );
        provider
            .fetch_authorized_table_scan_tasks(
                capability,
                ProviderFetchScanTasksRequest {
                    plan_task: request.plan_task,
                },
            )
            .await
            .map_err(catalog_provider_error)?
    };
    #[cfg(not(feature = "sail-local"))]
    let fetched = state
        .sail
        .fetch_scan_tasks(SailFetchScanTasksRequest {
            table: capability.table().clone(),
            principal: capability.receipt().principal.clone(),
            metadata_location: table.metadata_location.clone(),
            table_metadata: table.metadata.clone(),
            plan_task: request.plan_task,
            required_projection: restriction.effective_projection(&[])?,
            required_filters: restriction.mandatory_filters(),
        })
        .await?;
    Ok(fetched)
}

pub(crate) fn plan_task_tokens(tasks: &[serde_json::Value]) -> Vec<String> {
    tasks
        .iter()
        .filter_map(|task| task.get("plan-task").and_then(serde_json::Value::as_str))
        .map(ToString::to_string)
        .collect()
}

pub(crate) fn merge_scan_request_extensions(
    residual_filter: Option<serde_json::Value>,
    extensions: serde_json::Value,
) -> Option<serde_json::Value> {
    merge_lakecat_residual_extension(residual_filter, "lakecat:scan-request", extensions)
}

pub(crate) fn fetch_scan_tasks_extensions(
    capability: &TableScanCapability,
) -> Result<serde_json::Value, LakeCatHttpError> {
    let restriction = capability.read_restriction();
    let required_projection = restriction.effective_projection(&[])?;
    let required_filters = restriction.mandatory_filters();
    let stats_fields = required_projection.clone();
    Ok(json!({
        "read-restriction": restriction,
        "required-projection": required_projection.clone(),
        "effective-projection": required_projection,
        "required-filters": required_filters,
        "requested-stats-fields": stats_fields.clone(),
        "effective-stats-fields": stats_fields.clone(),
        "stats-fields": stats_fields,
    }))
}

pub(crate) fn merge_fetch_scan_tasks_extensions(
    residual_filter: Option<serde_json::Value>,
    extensions: serde_json::Value,
) -> Option<serde_json::Value> {
    merge_lakecat_residual_extension(residual_filter, "lakecat:fetch-scan-tasks", extensions)
}

pub(crate) fn merge_lakecat_residual_extension(
    residual_filter: Option<serde_json::Value>,
    extension_key: &str,
    extensions: serde_json::Value,
) -> Option<serde_json::Value> {
    match residual_filter {
        Some(mut residual @ serde_json::Value::Object(_)) => {
            residual[extension_key] = extensions;
            Some(residual)
        }
        Some(residual) => {
            let mut object = serde_json::Map::new();
            object.insert("lakecat:residual-filter".to_string(), residual);
            object.insert(extension_key.to_string(), extensions);
            Some(serde_json::Value::Object(object))
        }
        None => {
            let mut object = serde_json::Map::new();
            object.insert(extension_key.to_string(), extensions);
            Some(serde_json::Value::Object(object))
        }
    }
}

pub(crate) async fn querygraph_bootstrap(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<QueryGraphBootstrap>, LakeCatHttpError> {
    let capability = authorize_graph_read(&state, request_identity(&headers)?).await?;
    let tables = state.store.list_tables(&state.warehouse).await?;
    let table_idents = tables
        .iter()
        .map(|table| table.ident.clone())
        .collect::<Vec<_>>();
    let policy_bindings = state
        .store
        .policy_bindings_for_tables(&table_idents)
        .await?;
    if policy_bindings.len() != tables.len() {
        return Err(LakeCatError::Internal(
            "bulk policy lookup returned the wrong number of table scopes".to_string(),
        )
        .into());
    }
    let policy_binding_count = policy_bindings.iter().map(Vec::len).sum::<usize>();
    let table_policy_bindings = tables.into_iter().zip(policy_bindings);
    let views = state.store.list_warehouse_views(&state.warehouse).await?;
    let tenant = querygraph_tenant_projection(&state).await?;
    let view_receipts = state
        .store
        .list_warehouse_view_version_receipts(&state.warehouse)
        .await?;
    let view_version_receipts = querygraph_view_version_receipts(&views, &view_receipts)?;
    let bundle = lakecat_querygraph::bootstrap_from_catalog_snapshot(
        state.warehouse.clone(),
        table_policy_bindings,
        views,
        tenant,
        view_version_receipts,
    )?;
    let verification = bundle.construction_summary()?;
    #[cfg(debug_assertions)]
    {
        let independently_verified = bundle.verify_manifest()?;
        debug_assert_eq!(verification, independently_verified);
    }
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "querygraph.bootstrap",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "querygraph.bootstrap",
                "authorization-receipt": capability.receipt(),
                "warehouse": state.warehouse.as_str(),
                "table-count": verification.table_count,
                "view-count": verification.view_count,
                "policy-binding-count": policy_binding_count,
                "verified-tables": verification.verified_tables,
                "verified-views": verification.verified_views,
                "verified-view-versions": verification.verified_view_versions,
                "view-version-receipts": verification
                    .verified_view_receipt_hashes
                    .iter()
                    .map(|(stable_id, receipt_hash)| json!({
                        "stable-id": stable_id,
                        "view-version": verification
                            .verified_view_versions
                            .get(stable_id)
                            .copied()
                            .unwrap_or_default(),
                        "receipt-hash": receipt_hash,
                        "receipt-chain-hash": verification
                            .verified_view_receipt_chain_hashes
                            .get(stable_id),
                    }))
                    .collect::<Vec<_>>(),
                "bundle-hash": verification.bundle_hash,
                "graph-hash": verification.graph_hash,
                "open-lineage-hash": verification.open_lineage_hash,
                "querygraph-import-hash": verification.querygraph_import_hash,
                "table-artifacts": &bundle.manifest.table_artifacts,
                "view-artifacts": &bundle.manifest.view_artifacts,
                "standards": verification.standards,
            }),
        )?)
        .await?;
    Ok(Json(bundle))
}

pub(crate) async fn querygraph_tenant_projection(
    state: &LakeCatState,
) -> LakeCatResult<QueryGraphTenantProjection> {
    let warehouse_record = optional_management_record(
        state.store.load_warehouse(&state.warehouse).await,
        "warehouse",
    )?;
    let project_record = match warehouse_record.as_ref() {
        Some(warehouse) => optional_management_record(
            state.store.load_project(&warehouse.project_id).await,
            "project",
        )?,
        None => None,
    };
    let server_record = match project_record
        .as_ref()
        .and_then(|project| project.server_id.as_ref())
    {
        Some(server_id) => {
            optional_management_record(state.store.load_server(server_id).await, "server")?
        }
        None => None,
    };
    Ok(lakecat_querygraph::tenant_projection_from_records(
        &state.warehouse,
        warehouse_record.as_ref(),
        project_record.as_ref(),
        server_record.as_ref(),
    ))
}

fn optional_management_record<T>(
    result: LakeCatResult<T>,
    object: &'static str,
) -> LakeCatResult<Option<T>> {
    match result {
        Ok(record) => Ok(Some(record)),
        Err(LakeCatError::NotFound {
            object: missing_object,
            ..
        }) if missing_object == object => Ok(None),
        Err(err) => Err(err),
    }
}

pub(crate) fn querygraph_view_version_receipts(
    views: &[ViewRecord],
    receipts: &[ViewVersionReceipt],
) -> LakeCatResult<Vec<QueryGraphViewReceiptEvidence>> {
    let mut receipt_chains = HashMap::with_capacity(views.len());
    for receipt in receipts {
        receipt_chains
            .entry((&receipt.warehouse, &receipt.namespace, &receipt.name))
            .or_insert_with(Vec::new)
            .push(receipt);
    }
    let mut evidence = Vec::with_capacity(views.len());
    for view in views {
        let version_receipts = receipt_chains
            .get(&(&view.warehouse, &view.namespace, &view.name))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let response_receipts = version_receipts
            .iter()
            .copied()
            .map(view_version_receipt_response)
            .collect::<LakeCatResult<Vec<_>>>()?;
        if !view_version_receipt_chain_verified(&response_receipts) {
            return Err(LakeCatError::Internal(format!(
                "querygraph bootstrap view {}.{} has an unverified receipt chain",
                view.namespace.path(),
                view.name.as_str()
            )));
        }
        let receipt_chain_hash = view_version_receipt_chain_hash(&response_receipts)?;
        if let Some(receipt) = response_receipts
            .iter()
            .rev()
            .find(|receipt| receipt.view_version == view.view_version)
        {
            evidence.push(QueryGraphViewReceiptEvidence {
                stable_id: receipt.stable_id.clone(),
                view_version: receipt.view_version,
                receipt_hash: receipt.receipt_hash.clone(),
                receipt_chain_hash,
            });
        } else {
            return Err(LakeCatError::Internal(format!(
                "querygraph bootstrap view {}.{} is missing receipt evidence for version {}",
                view.namespace.path(),
                view.name.as_str(),
                view.view_version
            )));
        }
    }
    Ok(evidence)
}
