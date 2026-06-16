use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use lakecat_api::{
    CatalogConfigResponse, CommitTableRequest, CommitTableResponse, CreateNamespaceRequest,
    CreateTableRequest, FetchScanTasksRequest as ApiFetchScanTasksRequest, FetchScanTasksResponse,
    ListNamespacesResponse, LoadTableResponse, NamespaceResponse, PlanTableScanRequest,
    PlanTableScanResponse, TableIdentifier,
};
use lakecat_core::{
    LakeCatError, Namespace, Principal, PrincipalKind, TableIdent, TableName, WarehouseName,
    content_hash_bytes,
};
use lakecat_graph::{CatalogGraphSink, GraphAction, GraphEvent, NoopCatalogGraphSink};
use lakecat_lineage::{HashOnlyLineageSink, LineageEvent, LineageEventType, LineageSink};
use lakecat_querygraph::QueryGraphBootstrap;
#[cfg(not(feature = "sail-local"))]
use lakecat_sail::DeferredSailCatalogEngine;
use lakecat_sail::{
    CommitPreparationRequest, FetchScanTasksRequest as SailFetchScanTasksRequest,
    SailCatalogEngine, ScanPlanningRequest,
};
use lakecat_security::{
    AllowAllGovernanceEngine, AuthorizationReceipt, AuthorizationRequest, CatalogAction,
    GovernanceEngine, TableScanCapability,
};
use lakecat_store::{CatalogAuditEvent, CatalogStore, TableCommit, TableRecord, table_ident};
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStoreExt, PutPayload};
use serde_json::json;
use url::Url;

#[derive(Clone)]
pub struct LakeCatState {
    pub warehouse: WarehouseName,
    pub store: Arc<dyn CatalogStore>,
    pub sail: Arc<dyn SailCatalogEngine>,
    pub governance: Arc<dyn GovernanceEngine>,
    pub graph: Arc<dyn CatalogGraphSink>,
    pub lineage: Arc<dyn LineageSink>,
}

impl LakeCatState {
    pub fn new(warehouse: WarehouseName, store: Arc<dyn CatalogStore>) -> Self {
        Self {
            warehouse,
            store,
            sail: default_sail_engine(),
            governance: AllowAllGovernanceEngine::new(),
            graph: NoopCatalogGraphSink::new(),
            lineage: HashOnlyLineageSink::new(),
        }
    }

    pub fn with_integrations(
        mut self,
        sail: Arc<dyn SailCatalogEngine>,
        governance: Arc<dyn GovernanceEngine>,
        graph: Arc<dyn CatalogGraphSink>,
        lineage: Arc<dyn LineageSink>,
    ) -> Self {
        self.sail = sail;
        self.governance = governance;
        self.graph = graph;
        self.lineage = lineage;
        self
    }
}

#[cfg(feature = "sail-local")]
fn default_sail_engine() -> Arc<dyn SailCatalogEngine> {
    lakecat_sail::sail_integration::SailRestModelCatalogEngine::new()
}

#[cfg(not(feature = "sail-local"))]
fn default_sail_engine() -> Arc<dyn SailCatalogEngine> {
    DeferredSailCatalogEngine::new()
}

pub fn app(state: LakeCatState) -> Router {
    Router::new()
        .route("/catalog/v1/config", get(get_config))
        .route(
            "/catalog/v1/namespaces",
            get(list_namespaces).post(create_namespace),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables",
            post(create_table),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}",
            get(load_table),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/commit",
            post(commit_table),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/plan",
            post(plan_table_scan),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/fetch-scan-tasks",
            post(fetch_scan_tasks),
        )
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/tasks",
            post(fetch_scan_tasks),
        )
        .route("/querygraph/v1/bootstrap", get(querygraph_bootstrap))
        .with_state(state)
}

async fn get_config(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<CatalogConfigResponse>, LakeCatHttpError> {
    authorize(
        &state,
        request_principal(&headers)?,
        CatalogAction::CatalogConfig,
        None,
    )
    .await?;
    Ok(Json(CatalogConfigResponse::default()))
}

async fn create_namespace(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Json(request): Json<CreateNamespaceRequest>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let principal = request_principal(&headers)?;
    authorize(
        &state,
        principal.clone(),
        CatalogAction::NamespaceCreate,
        None,
    )
    .await?;
    let namespace = Namespace::new(request.namespace)?;
    state
        .store
        .create_namespace(&state.warehouse, namespace.clone())
        .await?;
    state
        .lineage
        .emit(LineageEvent::new(
            LineageEventType::NamespaceCreated,
            principal,
            None,
            json!({ "namespace": namespace.parts(), "warehouse": state.warehouse.as_str() }),
        ))
        .await?;
    Ok(Json(NamespaceResponse::from_namespace(&namespace)))
}

async fn list_namespaces(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListNamespacesResponse>, LakeCatHttpError> {
    authorize(
        &state,
        request_principal(&headers)?,
        CatalogAction::NamespaceList,
        None,
    )
    .await?;
    let namespaces = state.store.list_namespaces(&state.warehouse).await?;
    Ok(Json(ListNamespacesResponse {
        namespaces: namespaces
            .into_iter()
            .map(|namespace| namespace.parts().to_vec())
            .collect(),
    }))
}

async fn create_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(namespace): Path<String>,
    Json(request): Json<CreateTableRequest>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let principal = request_principal(&headers)?;
    let ident = table_ident(
        state.warehouse.as_str(),
        namespace,
        TableName::new(request.name)?.as_str(),
    )?;
    authorize(
        &state,
        principal.clone(),
        CatalogAction::TableCreate,
        Some(ident.clone()),
    )
    .await?;
    let table = TableRecord::new(
        ident.clone(),
        request.location,
        request.metadata_location,
        request.metadata,
        principal.clone(),
    );
    let table = state.store.create_table(table).await?;
    state
        .graph
        .emit(GraphEvent::table(
            GraphAction::Created,
            ident.clone(),
            json!({ "warehouse": state.warehouse.as_str() }),
        ))
        .await?;
    state
        .lineage
        .emit(LineageEvent::new(
            LineageEventType::TableCreated,
            principal,
            Some(ident),
            json!({ "metadata_location": table.metadata_location }),
        ))
        .await?;
    Ok(Json(load_table_response(table)))
}

async fn load_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let principal = request_principal(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
    authorize(
        &state,
        principal.clone(),
        CatalogAction::TableLoad,
        Some(ident.clone()),
    )
    .await?;
    let table = state.store.load_table(&ident).await?;
    state
        .graph
        .emit(GraphEvent::table(
            GraphAction::Loaded,
            ident.clone(),
            json!({ "metadata_location": table.metadata_location }),
        ))
        .await?;
    state
        .lineage
        .emit(LineageEvent::new(
            LineageEventType::TableLoaded,
            principal,
            Some(ident),
            json!({ "version": table.version }),
        ))
        .await?;
    Ok(Json(load_table_response(table)))
}

async fn commit_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<CommitTableRequest>,
) -> Result<Json<CommitTableResponse>, LakeCatHttpError> {
    let principal = request_principal(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
    let authorization_receipt = authorize(
        &state,
        principal.clone(),
        CatalogAction::TableCommit,
        Some(ident.clone()),
    )
    .await?;
    let current = state.store.load_table(&ident).await?;
    let current_metadata_location = current.metadata_location.clone();
    let commit_plan = state
        .sail
        .prepare_commit(CommitPreparationRequest {
            table: ident.clone(),
            principal: principal.clone(),
            current_metadata_location: current_metadata_location.clone(),
            new_metadata_location: request.metadata_location,
            current_metadata: current.metadata,
            new_metadata: request.metadata,
            requirements: request.requirements,
            updates: request.updates,
        })
        .await?;
    write_planned_metadata(&commit_plan).await?;
    let table = state
        .store
        .commit_table(
            &ident,
            TableCommit {
                requirements: commit_plan.requirements,
                updates: commit_plan.updates,
                expected_previous_metadata_location: current_metadata_location.clone(),
                new_metadata_location: commit_plan.new_metadata_location.clone(),
                new_metadata: Some(commit_plan.new_metadata.clone()),
                idempotency_key: None,
                principal: principal.clone(),
                authorization_receipt: Some(serde_json::to_value(&authorization_receipt).map_err(
                    |err| {
                        LakeCatError::Internal(format!(
                            "failed to encode authorization receipt: {err}"
                        ))
                    },
                )?),
            },
        )
        .await?;
    state
        .graph
        .emit(GraphEvent::table(
            GraphAction::Committed,
            ident.clone(),
            json!({ "version": table.version, "prepared_by": commit_plan.prepared_by }),
        ))
        .await?;
    state
        .lineage
        .emit(LineageEvent::new(
            LineageEventType::TableCommitted,
            principal,
            Some(ident),
            json!({ "version": table.version, "sail": commit_plan.metadata_patch }),
        ))
        .await?;
    Ok(Json(CommitTableResponse {
        metadata_location: table.metadata_location,
        metadata: table.metadata,
    }))
}

async fn write_planned_metadata(
    commit_plan: &lakecat_sail::CommitPlan,
) -> Result<(), LakeCatError> {
    if !commit_plan.metadata_write_required {
        return Ok(());
    }
    let Some(location) = commit_plan.new_metadata_location.as_deref() else {
        return Ok(());
    };
    let url = Url::parse(location).map_err(|err| {
        LakeCatError::InvalidArgument(format!("invalid metadata location '{location}': {err}"))
    })?;
    if url.scheme() != "file" {
        return Err(LakeCatError::NotSupported(format!(
            "metadata object writes currently support file:// locations, not {}",
            url.scheme()
        )));
    }
    let path = url.to_file_path().map_err(|_| {
        LakeCatError::InvalidArgument(format!(
            "metadata location is not a valid file URL: {location}"
        ))
    })?;
    let object_path = ObjectPath::from_absolute_path(&path).map_err(|err| {
        LakeCatError::InvalidArgument(format!("invalid metadata object path '{location}': {err}"))
    })?;
    let payload = serde_json::to_vec_pretty(&commit_plan.new_metadata)
        .map_err(|err| LakeCatError::Internal(format!("failed to encode metadata JSON: {err}")))?;
    let store = LocalFileSystem::new();
    store
        .put(&object_path, PutPayload::from(payload))
        .await
        .map_err(|err| {
            LakeCatError::Internal(format!(
                "failed to write metadata object '{location}': {err}"
            ))
        })?;
    Ok(())
}

async fn plan_table_scan(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<PlanTableScanRequest>,
) -> Result<Json<PlanTableScanResponse>, LakeCatHttpError> {
    let principal = request_principal(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_scan(&state, principal.clone(), ident.clone()).await?;
    let table = state.store.load_table(capability.table()).await?;
    let (scan, scan_request_extensions) =
        plan_scan_with_capability(&state, &capability, table, request).await?;
    let ident = capability.table().clone();
    let principal = capability.receipt().principal.clone();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.scan-planned",
            Some(ident.clone()),
            principal.clone(),
            json!({
                "event-type": "table.scan-planned",
                "table": ident,
                "authorization-receipt": capability.receipt(),
                "planned-by": scan.planned_by,
                "snapshot-id": scan.snapshot_id,
                "scan-task-count": scan.scan_tasks.len(),
            }),
        )?)
        .await?;
    state
        .graph
        .emit(GraphEvent::table(
            GraphAction::PlannedScan,
            ident.clone(),
            json!({ "planned_by": scan.planned_by, "snapshot_id": scan.snapshot_id }),
        ))
        .await?;
    state
        .lineage
        .emit(LineageEvent::new(
            LineageEventType::TableScanPlanned,
            principal,
            Some(ident.clone()),
            json!({ "planned_by": scan.planned_by, "scan_tasks": scan.scan_tasks.len() }),
        ))
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
    }))
}

async fn plan_scan_with_capability(
    state: &LakeCatState,
    capability: &TableScanCapability,
    table: TableRecord,
    request: PlanTableScanRequest,
) -> Result<(lakecat_sail::ScanPlan, serde_json::Value), LakeCatHttpError> {
    request.validate_scan_mode()?;
    let projection = request.projected_fields();
    let filters = request.filter_values();
    let scan_request_extensions = json!({
        "case-sensitive": request.case_sensitive,
        "use-snapshot-schema": request.use_snapshot_schema,
        "start-snapshot-id": request.start_snapshot_id,
        "end-snapshot-id": request.end_snapshot_id,
        "stats-fields": request.stats_fields,
    });
    let scan = state
        .sail
        .plan_scan(ScanPlanningRequest {
            table: capability.table().clone(),
            principal: capability.receipt().principal.clone(),
            metadata_location: table.metadata_location.clone(),
            table_metadata: table.metadata.clone(),
            projection,
            filters,
            limit: request.limit,
            snapshot_id: request.snapshot_id,
            start_snapshot_id: request.start_snapshot_id,
            end_snapshot_id: request.end_snapshot_id,
        })
        .await?;
    Ok((scan, scan_request_extensions))
}

async fn fetch_scan_tasks(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<ApiFetchScanTasksRequest>,
) -> Result<Json<FetchScanTasksResponse>, LakeCatHttpError> {
    let principal = request_principal(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_scan(&state, principal, ident).await?;
    let table = state.store.load_table(capability.table()).await?;
    let fetched = fetch_scan_tasks_with_capability(&state, &capability, table, request).await?;
    let ident = capability.table().clone();
    Ok(Json(FetchScanTasksResponse {
        table: TableIdentifier::from_ident(&ident),
        planned_by: fetched.planned_by,
        plan_task: fetched.plan_task,
        snapshot_id: fetched.snapshot_id,
        file_scan_tasks: fetched.file_scan_tasks,
        delete_files: fetched.delete_files,
        plan_tasks: plan_task_tokens(&fetched.plan_tasks),
        lakecat_plan_tasks: fetched.plan_tasks,
        residual_filter: fetched.residual_filter,
    }))
}

async fn fetch_scan_tasks_with_capability(
    state: &LakeCatState,
    capability: &TableScanCapability,
    table: TableRecord,
    request: ApiFetchScanTasksRequest,
) -> Result<lakecat_sail::FetchScanTasksPlan, LakeCatHttpError> {
    Ok(state
        .sail
        .fetch_scan_tasks(SailFetchScanTasksRequest {
            table: capability.table().clone(),
            principal: capability.receipt().principal.clone(),
            metadata_location: table.metadata_location,
            table_metadata: table.metadata,
            plan_task: request.plan_task,
        })
        .await?)
}

fn plan_task_tokens(tasks: &[serde_json::Value]) -> Vec<String> {
    tasks
        .iter()
        .filter_map(|task| task.get("plan-task").and_then(serde_json::Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn merge_scan_request_extensions(
    residual_filter: Option<serde_json::Value>,
    extensions: serde_json::Value,
) -> Option<serde_json::Value> {
    match residual_filter {
        Some(mut residual @ serde_json::Value::Object(_)) => {
            residual["lakecat:scan-request"] = extensions;
            Some(residual)
        }
        Some(residual) => Some(json!({
            "lakecat:residual-filter": residual,
            "lakecat:scan-request": extensions,
        })),
        None => Some(json!({
            "lakecat:scan-request": extensions,
        })),
    }
}

async fn querygraph_bootstrap(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<QueryGraphBootstrap>, LakeCatHttpError> {
    authorize(
        &state,
        request_principal(&headers)?,
        CatalogAction::GraphRead,
        None,
    )
    .await?;
    let tables = state.store.list_tables(&state.warehouse).await?;
    Ok(Json(QueryGraphBootstrap::from_tables(
        state.warehouse.clone(),
        tables,
    )?))
}

fn load_table_response(table: TableRecord) -> LoadTableResponse {
    LoadTableResponse {
        identifier: TableIdentifier::from_ident(&table.ident),
        metadata_location: table.metadata_location,
        metadata: table.metadata,
        config: vec![],
    }
}

fn request_principal(headers: &HeaderMap) -> Result<Principal, LakeCatHttpError> {
    let header = |name: &str| -> Result<Option<&str>, LakeCatError> {
        headers
            .get(name)
            .map(|value| {
                value.to_str().map_err(|_| {
                    LakeCatError::InvalidArgument(format!("invalid UTF-8 in {name} header"))
                })
            })
            .transpose()
    };

    if let Some(subject) = header("x-lakecat-principal")? {
        let kind = header("x-lakecat-principal-kind")?
            .map(str::parse)
            .transpose()?
            .unwrap_or(PrincipalKind::Human);
        return Principal::new(subject, kind).map_err(Into::into);
    }

    if let Some(did) = header("x-lakecat-agent-did")? {
        return Principal::new(did, PrincipalKind::Agent).map_err(Into::into);
    }

    if let Some(authorization) = header("authorization")? {
        if let Some(token) = authorization.strip_prefix("Bearer ") {
            let subject = format!("bearer:{}", content_hash_bytes(token.as_bytes()));
            return Principal::new(subject, PrincipalKind::Service).map_err(Into::into);
        }
        return Err(LakeCatError::InvalidArgument(
            "unsupported Authorization scheme; use Bearer".to_string(),
        )
        .into());
    }

    Ok(Principal::anonymous())
}

async fn authorize(
    state: &LakeCatState,
    principal: Principal,
    action: CatalogAction,
    table: Option<TableIdent>,
) -> Result<AuthorizationReceipt, LakeCatHttpError> {
    let receipt = state
        .governance
        .authorize(AuthorizationRequest {
            principal,
            action,
            table,
            context: json!({ "warehouse": state.warehouse.as_str() }),
        })
        .await?;
    if receipt.allowed {
        Ok(receipt)
    } else {
        Err(LakeCatError::Conflict("authorization denied".to_string()).into())
    }
}

async fn authorize_table_scan(
    state: &LakeCatState,
    principal: Principal,
    table: TableIdent,
) -> Result<TableScanCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        principal,
        CatalogAction::TablePlanScan,
        Some(table.clone()),
    )
    .await?;
    Ok(TableScanCapability::from_receipt(receipt, table)?)
}

#[derive(Debug)]
pub struct LakeCatHttpError(LakeCatError);

impl From<LakeCatError> for LakeCatHttpError {
    fn from(value: LakeCatError) -> Self {
        Self(value)
    }
}

impl IntoResponse for LakeCatHttpError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            LakeCatError::InvalidArgument(_) => StatusCode::BAD_REQUEST,
            LakeCatError::NotFound { .. } => StatusCode::NOT_FOUND,
            LakeCatError::Conflict(_) => StatusCode::CONFLICT,
            LakeCatError::NotSupported(_) => StatusCode::NOT_IMPLEMENTED,
            LakeCatError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(json!({
            "error": {
                "message": self.0.to_string(),
                "type": "LakeCatError",
                "code": status.as_u16()
            }
        }));
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use axum::body::Body;
    use http::{Method, Request, StatusCode};
    use lakecat_store::MemoryCatalogStore;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    #[derive(Debug, Default)]
    struct RecordingGovernance {
        principals: Mutex<Vec<Principal>>,
    }

    #[async_trait]
    impl GovernanceEngine for RecordingGovernance {
        async fn authorize(
            &self,
            request: AuthorizationRequest,
        ) -> lakecat_core::LakeCatResult<lakecat_security::AuthorizationReceipt> {
            self.principals.lock().await.push(request.principal.clone());
            Ok(lakecat_security::AuthorizationReceipt {
                principal: request.principal,
                action: request.action,
                table: request.table,
                allowed: true,
                engine: "recording".to_string(),
                policy_hash: None,
                checked_at: chrono::Utc::now(),
            })
        }
    }

    #[tokio::test]
    async fn config_endpoint_reports_lakecat_capabilities() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn list_namespaces_does_not_fabricate_default_namespace() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/namespaces")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["namespaces"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn authorization_headers_resolve_typed_principal() {
        let governance = Arc::new(RecordingGovernance::default());
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        )
        .with_integrations(
            default_sail_engine(),
            governance.clone(),
            NoopCatalogGraphSink::new(),
            HashOnlyLineageSink::new(),
        ));
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/catalog/v1/config")
                    .header("x-lakecat-principal", "alice@example.com")
                    .header("x-lakecat-principal-kind", "human")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let principals = governance.principals.lock().await;
        assert_eq!(principals[0].subject, "alice@example.com");
        assert_eq!(principals[0].kind, PrincipalKind::Human);
    }

    #[tokio::test]
    async fn create_load_commit_and_plan_table_round_trips_through_integrations() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true,"doc":"Event identifier."}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"requirements":[],"updates":[]}"#))
            .unwrap();
        let response = app.clone().oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let plan = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"select":["id"],"filter":{"type":"always-true"},"case-sensitive":true,"limit":10}"#))
            .unwrap();
        let response = app.clone().oneshot(plan).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["status"], serde_json::json!("completed"));
        let _: sail_catalog_iceberg::models::PlanTableScanRequest =
            serde_json::from_value(serde_json::json!({
                "select": ["id"],
                "filter": {"type": "always-true"},
                "case-sensitive": true
            }))
            .unwrap();
        assert_eq!(
            payload["residual-filter"]["lakecat:scan-request"]["case-sensitive"],
            serde_json::json!(true)
        );
        #[cfg(feature = "sail-local")]
        {
            assert_eq!(
                payload["lakecat-plan-tasks"][0]["task-type"],
                serde_json::json!("manifest-list")
            );
            assert_eq!(
                payload["residual-filter"]["filters-accepted-by-sail"][0]["expression-type"],
                serde_json::json!("always-true")
            );
            let plan_task = payload["plan-tasks"][0]
                .as_str()
                .expect("plan task token")
                .to_string();

            let fetch = Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces/default/tables/events/fetch-scan-tasks")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "plan-task": plan_task }).to_string(),
                ))
                .unwrap();
            let response = app.oneshot(fetch).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let _: sail_catalog_iceberg::models::FetchScanTasksResult =
                serde_json::from_value(payload.clone()).unwrap();
            assert_eq!(
                payload["residual-filter"]["lakecat:sail-target"],
                serde_json::json!("sail_iceberg::io::load_manifest_list")
            );
        }
    }

    #[tokio::test]
    async fn commit_can_advance_metadata_location_extension() {
        let app = test_app();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-commit-metadata-{unique}"));
        let table_dir = root.join("events");
        let metadata_dir = table_dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let table_location = url::Url::from_directory_path(&table_dir)
            .expect("table dir URL")
            .to_string();
        let initial_metadata_location = url::Url::from_file_path(metadata_dir.join("00000.json"))
            .unwrap()
            .to_string();
        let committed_metadata_location = url::Url::from_file_path(metadata_dir.join("00001.json"))
            .unwrap()
            .to_string();
        let new_metadata = serde_json::json!({
            "format-version": 3,
            "table-uuid": "11111111-1111-1111-1111-111111111111",
            "location": table_location,
            "last-sequence-number": 8,
            "last-updated-ms": 1710000000100_i64,
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
            "current-snapshot-id": 43,
            "snapshots": [{
                "snapshot-id": 43,
                "sequence-number": 8,
                "timestamp-ms": 1710000000100_i64,
                "summary": {"operation": "append"},
                "schema-id": 1
            }],
            "snapshot-log": [{"timestamp-ms": 1710000000100_i64, "snapshot-id": 43}]
        });
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "name": "events",
                    "location": table_location,
                    "metadata-location": initial_metadata_location,
                    "metadata": {
                        "format-version": 3,
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
                            "summary": {"operation": "append"},
                            "schema-id": 1
                        }],
                        "snapshot-log": [{"timestamp-ms": 1710000000000_i64, "snapshot-id": 42}]
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "requirements": [],
                    "updates": [],
                    "metadata-location": committed_metadata_location,
                    "metadata": new_metadata,
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload["metadata-location"],
            serde_json::json!(committed_metadata_location)
        );
        let written_metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(metadata_dir.join("00001.json")).unwrap())
                .unwrap();
        assert_eq!(
            written_metadata["current-snapshot-id"],
            serde_json::json!(43)
        );

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload["metadata-location"],
            serde_json::json!(committed_metadata_location)
        );
        assert_eq!(
            payload["metadata"]["current-snapshot-id"],
            serde_json::json!(43)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "sail-local")]
    #[tokio::test]
    async fn fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens() {
        let fixture = local_manifest_fixture();
        let app = test_app();
        let create_payload = serde_json::json!({
            "name": "events",
            "location": fixture.table_location,
            "metadata-location": fixture.metadata_location,
            "metadata": fixture.metadata,
        });
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(create_payload.to_string()))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let plan = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "select": ["id"],
                    "filter": {"type": "always-true"},
                    "case-sensitive": true,
                    "stats-fields": ["id"]
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(plan).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let _: sail_catalog_iceberg::models::CompletedPlanningWithIdResult =
            serde_json::from_value(payload.clone()).unwrap();
        assert_eq!(payload["status"], serde_json::json!("completed"));
        assert_eq!(
            payload["residual-filter"]["lakecat:scan-request"]["stats-fields"][0],
            serde_json::json!("id")
        );
        assert_eq!(
            payload["residual-filter"]["filters-accepted-by-sail"][0]["filter"],
            serde_json::json!({"type": "always-true"})
        );
        let plan_task = payload["plan-tasks"][0]
            .as_str()
            .expect("plan task token")
            .to_string();

        let fetch = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/tasks")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "plan-task": plan_task }).to_string(),
            ))
            .unwrap();
        let response = app.oneshot(fetch).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let _: sail_catalog_iceberg::models::FileScanTask =
            serde_json::from_value(payload["file-scan-tasks"][0].clone()).unwrap();
        let _: sail_catalog_iceberg::models::PositionDeleteFile =
            serde_json::from_value(payload["delete-files"][0].clone()).unwrap();

        assert!(payload["plan-tasks"][0].as_str().is_some());
        assert_eq!(
            payload["lakecat-plan-tasks"][0]["task-type"],
            serde_json::json!("manifest")
        );
        assert_eq!(
            payload["file-scan-tasks"][0]["delete-file-references"][0],
            serde_json::json!(0)
        );
        assert_eq!(
            payload["delete-files"][0]["file-path"],
            serde_json::json!(fixture.delete_file_path)
        );

        let _ = std::fs::remove_dir_all(fixture.root);
    }

    #[tokio::test]
    async fn plan_rejects_invalid_incremental_scan_modes() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true,"doc":"Event identifier."}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let mixed = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "snapshot-id": 42,
                    "start-snapshot-id": 1,
                    "end-snapshot-id": 42
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(mixed).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let missing_end = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({"start-snapshot-id": 1}).to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(missing_end).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let missing_start = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({"end-snapshot-id": 42}).to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(missing_start).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        #[cfg(feature = "sail-local")]
        {
            let invalid_range = Request::builder()
                .method(Method::POST)
                .uri("/catalog/v1/namespaces/default/tables/events/plan")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "start-snapshot-id": 1,
                        "end-snapshot-id": 42
                    })
                    .to_string(),
                ))
                .unwrap();
            let response = app.clone().oneshot(invalid_range).await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }

        let valid_empty_delta = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "start-snapshot-id": 42,
                    "end-snapshot-id": 42
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.oneshot(valid_empty_delta).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["snapshot-id"], serde_json::json!(42));
        assert_eq!(body["plan-tasks"], serde_json::json!([]));
        assert_eq!(
            body["residual-filter"]["lakecat:scan-request"]["start-snapshot-id"],
            serde_json::json!(42)
        );
    }

    #[tokio::test]
    async fn querygraph_bootstrap_projects_catalog_tables() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true,"doc":"Event identifier.","semantic-type":"https://schema.org/identifier"}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bootstrap = Request::builder()
            .method(Method::GET)
            .uri("/querygraph/v1/bootstrap")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(bootstrap).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[cfg(feature = "sail-local")]
    #[tokio::test]
    async fn stale_commit_requirement_returns_conflict_with_sail_local_engine() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"table-uuid":"11111111-1111-1111-1111-111111111111","location":"file:///tmp/events","last-sequence-number":7,"last-updated-ms":1710000000000,"last-column-id":1,"schemas":[{"type":"struct","schema-id":1,"fields":[{"id":1,"name":"id","type":"string","required":true,"doc":"Event identifier."}]}],"current-schema-id":1,"partition-specs":[{"spec-id":0,"fields":[]}],"default-spec-id":0,"current-snapshot-id":42,"snapshots":[{"snapshot-id":42,"sequence-number":7,"timestamp-ms":1710000000000,"manifest-list":"file:///tmp/events/metadata/snap-42.avro","summary":{"operation":"append"},"schema-id":1}],"snapshot-log":[{"timestamp-ms":1710000000000,"snapshot-id":42}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let commit = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"requirements":[{"type":"assert-current-schema-id","current-schema-id":9}],"updates":[]}"#,
            ))
            .unwrap();
        let response = app.oneshot(commit).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    fn test_app() -> Router {
        app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        ))
    }

    #[cfg(feature = "sail-local")]
    struct LocalManifestFixture {
        root: std::path::PathBuf,
        table_location: String,
        metadata_location: String,
        delete_file_path: String,
        metadata: serde_json::Value,
    }

    #[cfg(feature = "sail-local")]
    fn local_manifest_fixture() -> LocalManifestFixture {
        use std::collections::HashMap;
        use std::sync::Arc;
        use std::time::{SystemTime, UNIX_EPOCH};

        use sail_iceberg::spec::{
            DataContentType, DataFile, DataFileFormat, FormatVersion, ManifestContentType,
            ManifestFile, ManifestListWriter, ManifestMetadata, ManifestWriterBuilder,
            TableMetadata,
        };
        use url::Url;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lakecat-service-manifest-{unique}"));
        let table_dir = root.join("table");
        let metadata_dir = table_dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();

        let table_location = Url::from_directory_path(&table_dir).unwrap().to_string();
        let manifest_list_path = metadata_dir.join("snap-42.avro");
        let manifest_list = Url::from_file_path(&manifest_list_path)
            .unwrap()
            .to_string();
        let metadata_location = format!("{table_location}metadata/00000.json");
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
        let metadata = serde_json::json!({
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
        let table_metadata =
            TableMetadata::from_json(&serde_json::to_vec(&metadata).unwrap()).unwrap();
        let data_manifest_metadata = ManifestMetadata::new(
            Arc::new(table_metadata.current_schema().unwrap().clone()),
            table_metadata.current_schema_id,
            table_metadata.default_partition_spec().unwrap().clone(),
            FormatVersion::V2,
            ManifestContentType::Data,
        );
        let mut data_writer =
            ManifestWriterBuilder::new(Some(42), None, data_manifest_metadata).build();
        data_writer.add(DataFile {
            content: DataContentType::Data,
            file_path: data_file_path,
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
            data_writer.to_avro_bytes_v2().unwrap(),
        )
        .unwrap();

        let delete_manifest_metadata = ManifestMetadata::new(
            Arc::new(table_metadata.current_schema().unwrap().clone()),
            table_metadata.current_schema_id,
            table_metadata.default_partition_spec().unwrap().clone(),
            FormatVersion::V2,
            ManifestContentType::Deletes,
        );
        let mut delete_writer =
            ManifestWriterBuilder::new(Some(42), None, delete_manifest_metadata).build();
        delete_writer.add(DataFile {
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
            referenced_data_file: Some(
                Url::from_file_path(table_dir.join("data").join("part-1.parquet"))
                    .unwrap()
                    .to_string(),
            ),
            content_offset: None,
            content_size_in_bytes: None,
        });
        std::fs::write(
            Url::parse(&delete_manifest_path)
                .unwrap()
                .to_file_path()
                .unwrap(),
            delete_writer.to_avro_bytes_v2().unwrap(),
        )
        .unwrap();

        let mut list_writer = ManifestListWriter::new();
        list_writer.append(
            ManifestFile::builder()
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
        std::fs::write(
            &manifest_list_path,
            list_writer.to_bytes(FormatVersion::V2).unwrap(),
        )
        .unwrap();

        LocalManifestFixture {
            root,
            table_location,
            metadata_location,
            delete_file_path,
            metadata,
        }
    }
}
