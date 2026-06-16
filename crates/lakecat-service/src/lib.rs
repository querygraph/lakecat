use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use lakecat_api::{
    CatalogConfigResponse, CommitTableRequest, CommitTableResponse, ConfigEntry,
    CreateNamespaceRequest, CreateTableRequest, FetchScanTasksRequest as ApiFetchScanTasksRequest,
    FetchScanTasksResponse, ListNamespacesResponse, ListPolicyBindingsResponse,
    ListStorageProfilesResponse, LoadCredentialsResponse, LoadTableResponse, NamespaceResponse,
    PlanTableScanRequest, PlanTableScanResponse, PolicyBindingResponse, StorageCredential,
    StorageProfileResponse, TableIdentifier, UpsertPolicyBindingRequest,
    UpsertStorageProfileRequest,
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
    CatalogConfigCapability, CredentialsVendCapability, GovernanceEngine, GraphReadCapability,
    NamespaceCreateCapability, NamespaceListCapability, PolicyManageCapability,
    StorageProfileManageCapability, TableCommitCapability, TableCreateCapability,
    TableDropCapability, TableLoadCapability, TableRestoreCapability, TableScanCapability,
};
use lakecat_store::{
    CatalogAuditEvent, CatalogStore, CredentialIssuanceMode, OutboxEvent, PolicyBinding,
    StorageProfile, StorageProvider, TableCommit, TableRecord, table_ident,
};
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStoreExt, PutPayload};
use serde_json::{Value, json};
use url::Url;

#[derive(Clone)]
pub struct LakeCatState {
    pub warehouse: WarehouseName,
    pub store: Arc<dyn CatalogStore>,
    pub sail: Arc<dyn SailCatalogEngine>,
    pub governance: Arc<dyn GovernanceEngine>,
    pub credential_issuer: Arc<dyn CredentialIssuer>,
    pub graph: Arc<dyn CatalogGraphSink>,
    pub lineage: Arc<dyn LineageSink>,
}

#[async_trait]
pub trait CredentialIssuer: Send + Sync + 'static {
    async fn issue(
        &self,
        request: CredentialIssuanceRequest,
    ) -> Result<Vec<StorageCredential>, LakeCatError>;
}

#[derive(Debug, Clone)]
pub struct CredentialIssuanceRequest {
    pub table: TableRecord,
    pub profile: StorageProfile,
    pub authorization_receipt: AuthorizationReceipt,
}

#[derive(Debug, Default)]
pub struct ConservativeCredentialIssuer;

impl ConservativeCredentialIssuer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl CredentialIssuer for ConservativeCredentialIssuer {
    async fn issue(
        &self,
        request: CredentialIssuanceRequest,
    ) -> Result<Vec<StorageCredential>, LakeCatError> {
        if request.profile.can_return_public_credential() {
            return Ok(public_storage_credentials_for_profile(&request.profile));
        }
        Ok(Vec::new())
    }
}

#[cfg(feature = "typesec-local")]
pub mod typesec_credential_issuer {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use lakecat_api::{ConfigEntry, StorageCredential};
    use lakecat_core::{LakeCatError, LakeCatResult};
    use lakecat_store::CredentialIssuanceMode;
    use typesec::{PolicyEngine, PolicyResult, ResourceId, SubjectId};

    use crate::{CredentialIssuanceRequest, CredentialIssuer};

    #[async_trait]
    pub trait SecretRefCredentialResolver: Send + Sync + 'static {
        async fn resolve(
            &self,
            request: &CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>>;
    }

    pub struct TypeSecCredentialIssuer {
        engine: Arc<dyn PolicyEngine>,
        resolver: Arc<dyn SecretRefCredentialResolver>,
    }

    impl TypeSecCredentialIssuer {
        pub fn new(
            engine: Arc<dyn PolicyEngine>,
            resolver: Arc<dyn SecretRefCredentialResolver>,
        ) -> Arc<Self> {
            Arc::new(Self { engine, resolver })
        }

        pub fn allow_all_demo() -> Arc<Self> {
            Self::new(Arc::new(AllowAllPolicy), Arc::new(NoopSecretRefResolver))
        }
    }

    #[async_trait]
    impl CredentialIssuer for TypeSecCredentialIssuer {
        async fn issue(
            &self,
            request: CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>> {
            if request.profile.can_return_public_credential() {
                return Ok(crate::public_storage_credentials_for_profile(
                    &request.profile,
                ));
            }
            if request.profile.issuance_mode != CredentialIssuanceMode::ShortLivedSecretRef {
                return Ok(Vec::new());
            }
            let Some(secret_ref) = request.profile.secret_ref.as_deref() else {
                return Err(LakeCatError::InvalidArgument(
                    "short-lived credential issuance requires a secret reference".to_string(),
                ));
            };
            if !secret_ref.starts_with("typesec://") {
                return Ok(Vec::new());
            }
            let subject = SubjectId::from(request.authorization_receipt.principal.subject.clone());
            let resource = ResourceId::from(secret_ref.to_string());
            let decision = self.engine.check(&subject, "credentials.issue", &resource);
            match decision {
                PolicyResult::Allow => self.resolver.resolve(&request).await,
                PolicyResult::Deny(reason) => Err(LakeCatError::Conflict(format!(
                    "TypeSec denied credential issuance: {reason}"
                ))),
                PolicyResult::Delegate(reason) => Err(LakeCatError::Conflict(format!(
                    "TypeSec delegated credential issuance without resolver policy: {reason}"
                ))),
                _ => Err(LakeCatError::Conflict(
                    "TypeSec returned an unsupported credential issuance decision".to_string(),
                )),
            }
        }
    }

    struct AllowAllPolicy;

    impl PolicyEngine for AllowAllPolicy {
        fn check(
            &self,
            _subject: &SubjectId,
            _action: &str,
            _resource: &ResourceId,
        ) -> PolicyResult {
            PolicyResult::Allow
        }
    }

    struct NoopSecretRefResolver;

    #[async_trait]
    impl SecretRefCredentialResolver for NoopSecretRefResolver {
        async fn resolve(
            &self,
            _request: &CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>> {
            Ok(Vec::new())
        }
    }

    pub struct StaticSecretRefCredentialResolver {
        credentials: BTreeMap<String, Vec<ConfigEntry>>,
    }

    impl StaticSecretRefCredentialResolver {
        pub fn new(credentials: BTreeMap<String, Vec<ConfigEntry>>) -> Arc<Self> {
            Arc::new(Self { credentials })
        }
    }

    #[async_trait]
    impl SecretRefCredentialResolver for StaticSecretRefCredentialResolver {
        async fn resolve(
            &self,
            request: &CredentialIssuanceRequest,
        ) -> LakeCatResult<Vec<StorageCredential>> {
            let Some(secret_ref) = request.profile.secret_ref.as_deref() else {
                return Ok(Vec::new());
            };
            let Some(config) = self.credentials.get(secret_ref) else {
                return Ok(Vec::new());
            };
            Ok(vec![StorageCredential {
                prefix: request.profile.location_prefix.clone(),
                config: config.clone(),
            }])
        }
    }
}

impl LakeCatState {
    pub fn new(warehouse: WarehouseName, store: Arc<dyn CatalogStore>) -> Self {
        Self {
            warehouse,
            store,
            sail: default_sail_engine(),
            governance: AllowAllGovernanceEngine::new(),
            credential_issuer: ConservativeCredentialIssuer::new(),
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

    pub fn with_credential_issuer(mut self, credential_issuer: Arc<dyn CredentialIssuer>) -> Self {
        self.credential_issuer = credential_issuer;
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
            get(load_table).delete(delete_table),
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
        .route(
            "/catalog/v1/namespaces/{namespace}/tables/{table}/credentials",
            get(load_credentials),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/namespaces/{namespace}/tables/{table}/restore",
            post(restore_table),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/storage-profiles",
            get(list_storage_profiles),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/storage-profiles/{profile}",
            post(upsert_storage_profile).put(upsert_storage_profile),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/policies",
            get(list_policy_bindings),
        )
        .route(
            "/management/v1/warehouses/{warehouse}/policies/{policy}",
            post(upsert_policy_binding).put(upsert_policy_binding),
        )
        .route("/querygraph/v1/bootstrap", get(querygraph_bootstrap))
        .with_state(state)
}

pub async fn drain_outbox_once(state: &LakeCatState, limit: usize) -> Result<usize, LakeCatError> {
    let events = state
        .store
        .pending_outbox_events(Some("lakecat.lineage-and-graph"), limit)
        .await?;
    let mut delivered = Vec::with_capacity(events.len());
    for event in events {
        project_outbox_event(state, &event).await?;
        delivered.push(event.event_id);
    }
    state.store.mark_outbox_delivered(&delivered).await
}

async fn get_config(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<CatalogConfigResponse>, LakeCatHttpError> {
    let capability = authorize_catalog_config(&state, request_identity(&headers)?).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "catalog.config-read",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "catalog.config-read",
                "authorization-receipt": capability.receipt(),
                "warehouse": state.warehouse.as_str(),
            }),
        )?)
        .await?;
    Ok(Json(CatalogConfigResponse::default()))
}

async fn create_namespace(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Json(request): Json<CreateNamespaceRequest>,
) -> Result<Json<NamespaceResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let capability = authorize_namespace_create(&state, identity).await?;
    let namespace = Namespace::new(request.namespace)?;
    state
        .store
        .create_namespace(&state.warehouse, namespace.clone())
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
                "warehouse": state.warehouse.as_str(),
                "namespace": namespace.parts(),
            }),
        )?)
        .await?;
    Ok(Json(NamespaceResponse::from_namespace(&namespace)))
}

async fn list_namespaces(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
) -> Result<Json<ListNamespacesResponse>, LakeCatHttpError> {
    let capability = authorize_namespace_list(&state, request_identity(&headers)?).await?;
    let namespaces = state.store.list_namespaces(&state.warehouse).await?;
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "namespace.listed",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "namespace.listed",
                "authorization-receipt": capability.receipt(),
                "warehouse": state.warehouse.as_str(),
                "namespace-count": namespaces.len(),
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

async fn create_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path(namespace): Path<String>,
    Json(request): Json<CreateTableRequest>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let principal = identity.principal.clone();
    let ident = table_ident(
        state.warehouse.as_str(),
        namespace,
        TableName::new(request.name)?.as_str(),
    )?;
    let capability = authorize_table_create(&state, identity, ident).await?;
    let ident = capability.table().clone();
    let table = TableRecord::new(
        ident.clone(),
        request.location,
        request.metadata_location,
        request.metadata,
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
                "version": table.version,
            }),
        )?)
        .await?;
    Ok(Json(load_table_response(table)))
}

async fn load_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<Json<LoadTableResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
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
                "version": table.version,
            }),
        )?)
        .await?;
    Ok(Json(load_table_response(table)))
}

async fn delete_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<StatusCode, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
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

async fn restore_table(
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

async fn load_credentials(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
) -> Result<Json<LoadCredentialsResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
    let capability = authorize_credentials_vend(&state, identity, ident).await?;
    let table = state.store.load_table(capability.table()).await?;
    let storage_profile = state.store.storage_profile_for_table(&table).await?;
    let storage_credentials = state
        .credential_issuer
        .issue(CredentialIssuanceRequest {
            table: table.clone(),
            profile: storage_profile.clone(),
            authorization_receipt: capability.receipt().clone(),
        })
        .await?;
    let ident = capability.table().clone();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "credentials.vend-attempted",
            Some(ident.clone()),
            capability.receipt().principal.clone(),
            json!({
                "event-type": "credentials.vend-attempted",
                "table": ident,
                "authorization-receipt": capability.receipt(),
                "storage-location": table.location,
                "storage-profile-id": storage_profile.profile_id,
                "secret-ref-present": storage_profile.secret_ref.is_some(),
                "credential-count": storage_credentials.len(),
                "mode": storage_profile.issuance_mode.as_str(),
            }),
        )?)
        .await?;
    Ok(Json(LoadCredentialsResponse {
        storage_credentials,
    }))
}

async fn list_storage_profiles(
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
            }),
        )?)
        .await?;
    Ok(Json(ListStorageProfilesResponse {
        storage_profiles: profiles.iter().map(storage_profile_response).collect(),
    }))
}

async fn upsert_storage_profile(
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
                "storage-profile": storage_profile_response(&storage_profile),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(storage_profile_response(&storage_profile)))
}

async fn list_policy_bindings(
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
            }),
        )?)
        .await?;
    Ok(Json(ListPolicyBindingsResponse {
        policies: policies.iter().map(policy_binding_response).collect(),
    }))
}

async fn upsert_policy_binding(
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
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "policy-binding.upserted",
            None,
            capability.receipt().principal.clone(),
            json!({
                "event-type": "policy-binding.upserted",
                "warehouse": warehouse.as_str(),
                "policy": policy_binding_response(&binding),
                "authorization-receipt": capability.receipt(),
            }),
        )?)
        .await?;
    Ok(Json(policy_binding_response(&binding)))
}

async fn commit_table(
    State(state): State<LakeCatState>,
    headers: HeaderMap,
    Path((namespace, table)): Path<(String, String)>,
    Json(request): Json<CommitTableRequest>,
) -> Result<Json<CommitTableResponse>, LakeCatHttpError> {
    let identity = request_identity(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_commit(&state, identity, ident).await?;
    let current = state.store.load_table(capability.table()).await?;
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
    write_planned_metadata(&commit_plan).await?;
    let table = state
        .store
        .commit_table(
            capability.table(),
            TableCommit {
                requirements: commit_plan.requirements,
                updates: commit_plan.updates,
                expected_previous_metadata_location: current_metadata_location.clone(),
                new_metadata_location: commit_plan.new_metadata_location.clone(),
                new_metadata: Some(commit_plan.new_metadata.clone()),
                idempotency_key: None,
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
    let identity = request_identity(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_scan(&state, identity, ident.clone()).await?;
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
    let identity = request_identity(&headers)?;
    let ident = table_ident(state.warehouse.as_str(), namespace, table)?;
    let capability = authorize_table_scan(&state, identity, ident).await?;
    let table = state.store.load_table(capability.table()).await?;
    let fetched = fetch_scan_tasks_with_capability(&state, &capability, table, request).await?;
    let ident = capability.table().clone();
    state
        .store
        .record_audit_event(CatalogAuditEvent::new(
            "table.scan-tasks-fetched",
            Some(ident.clone()),
            capability.receipt().principal.clone(),
            json!({
                "event-type": "table.scan-tasks-fetched",
                "table": ident,
                "authorization-receipt": capability.receipt(),
                "planned-by": fetched.planned_by,
                "snapshot-id": fetched.snapshot_id,
                "plan-task": fetched.plan_task,
                "file-scan-task-count": fetched.file_scan_tasks.len(),
                "delete-file-count": fetched.delete_files.len(),
                "child-plan-task-count": fetched.plan_tasks.len(),
            }),
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
    let capability = authorize_graph_read(&state, request_identity(&headers)?).await?;
    let tables = state.store.list_tables(&state.warehouse).await?;
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
                "table-count": tables.len(),
            }),
        )?)
        .await?;
    Ok(Json(QueryGraphBootstrap::from_tables(
        state.warehouse.clone(),
        tables,
    )?))
}

async fn project_outbox_event(
    state: &LakeCatState,
    event: &OutboxEvent,
) -> Result<(), LakeCatError> {
    let event_payload = event
        .payload
        .get("payload")
        .unwrap_or(&event.payload)
        .clone();
    let table = outbox_table(event)?;
    let principal = outbox_principal(event)?;
    if let Some((graph_action, lineage_type)) = outbox_table_projection(event.event_type.as_str()) {
        if let Some(table) = table.clone() {
            state
                .graph
                .emit(GraphEvent::table(
                    graph_action,
                    table.clone(),
                    event_payload.clone(),
                ))
                .await?;
            state
                .lineage
                .emit(LineageEvent::new(
                    lineage_type,
                    principal,
                    Some(table),
                    event_payload,
                ))
                .await?;
        }
    } else if event.event_type == "namespace.created" {
        state
            .lineage
            .emit(LineageEvent::new(
                LineageEventType::NamespaceCreated,
                principal,
                None,
                event_payload,
            ))
            .await?;
    } else if event.event_type == "table.restored" {
        if let Some(table) = table {
            state
                .lineage
                .emit(LineageEvent::new(
                    LineageEventType::TableRestored,
                    principal,
                    Some(table),
                    event_payload,
                ))
                .await?;
        }
    }
    Ok(())
}

fn outbox_table(event: &OutboxEvent) -> Result<Option<TableIdent>, LakeCatError> {
    event
        .payload
        .get("table")
        .filter(|table| !table.is_null())
        .map(|table| {
            serde_json::from_value(table.clone()).map_err(|err| {
                LakeCatError::Internal(format!("failed to decode outbox table: {err}"))
            })
        })
        .transpose()
}

fn outbox_principal(event: &OutboxEvent) -> Result<Principal, LakeCatError> {
    for pointer in [
        "/payload/authorization-receipt/principal",
        "/authorization-receipt/principal",
        "/commit/principal",
    ] {
        if let Some(principal) = event.payload.pointer(pointer) {
            return serde_json::from_value(principal.clone()).map_err(|err| {
                LakeCatError::Internal(format!("failed to decode outbox principal: {err}"))
            });
        }
    }
    Ok(Principal::anonymous())
}

fn outbox_table_projection(event_type: &str) -> Option<(GraphAction, LineageEventType)> {
    match event_type {
        "table.created" => Some((GraphAction::Created, LineageEventType::TableCreated)),
        "table.loaded" => Some((GraphAction::Loaded, LineageEventType::TableLoaded)),
        "table.scan-planned" => {
            Some((GraphAction::PlannedScan, LineageEventType::TableScanPlanned))
        }
        "table.commit" => Some((GraphAction::Committed, LineageEventType::TableCommitted)),
        "table.deleted" => Some((GraphAction::Deleted, LineageEventType::TableDeleted)),
        _ => None,
    }
}

fn load_table_response(table: TableRecord) -> LoadTableResponse {
    LoadTableResponse {
        identifier: TableIdentifier::from_ident(&table.ident),
        metadata_location: table.metadata_location,
        metadata: table.metadata,
        config: vec![],
    }
}

fn public_storage_credentials_for_profile(profile: &StorageProfile) -> Vec<StorageCredential> {
    let mut config = vec![ConfigEntry::new(
        "lakecat.storage-profile-id",
        profile.profile_id.clone(),
    )];
    config.extend(
        profile
            .public_config
            .iter()
            .map(|(key, value)| ConfigEntry::new(key.clone(), value.clone())),
    );
    vec![StorageCredential {
        prefix: profile.location_prefix.clone(),
        config,
    }]
}

fn storage_profile_response(profile: &StorageProfile) -> StorageProfileResponse {
    StorageProfileResponse {
        profile_id: profile.profile_id.clone(),
        warehouse: profile.warehouse.as_str().to_string(),
        location_prefix: profile.location_prefix.clone(),
        provider: profile.provider.as_str().to_string(),
        issuance_mode: profile.issuance_mode.as_str().to_string(),
        secret_ref: profile.secret_ref.clone(),
        public_config: profile.public_config.clone(),
    }
}

fn policy_binding_response(binding: &PolicyBinding) -> PolicyBindingResponse {
    PolicyBindingResponse {
        policy_id: binding.policy_id.clone(),
        warehouse: binding.warehouse.as_str().to_string(),
        namespace: binding
            .namespace
            .as_ref()
            .map(|namespace| namespace.parts().to_vec()),
        table: binding
            .table
            .as_ref()
            .map(|table| table.as_str().to_string()),
        enforced: binding.enforced,
        odrl: binding.odrl.clone(),
    }
}

fn management_warehouse(
    state: &LakeCatState,
    warehouse: String,
) -> Result<WarehouseName, LakeCatHttpError> {
    let warehouse = WarehouseName::new(warehouse)?;
    if warehouse != state.warehouse {
        return Err(LakeCatError::InvalidArgument(format!(
            "management warehouse {} does not match configured warehouse {}",
            warehouse, state.warehouse
        ))
        .into());
    }
    Ok(warehouse)
}

#[derive(Debug, Clone)]
struct RequestIdentity {
    principal: Principal,
    envelope: Value,
}

fn request_identity(headers: &HeaderMap) -> Result<RequestIdentity, LakeCatHttpError> {
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

    let explicit_principal = header("x-lakecat-principal")?;
    let explicit_kind = header("x-lakecat-principal-kind")?
        .map(str::parse)
        .transpose()?;
    let agent_did = header("x-lakecat-agent-did")?;
    let typedid = header("x-lakecat-typedid")?.or(agent_did);
    let typedid_proof = header("x-lakecat-typedid-proof")?;
    let delegation = header("x-lakecat-agent-delegation")?;
    let signed_summary = header("x-lakecat-agent-summary-signature")?;
    let authorization = header("authorization")?;

    let (principal, source, bearer_token_sha256) = if let Some(subject) = explicit_principal {
        (
            Principal::new(subject, explicit_kind.unwrap_or(PrincipalKind::Human))?,
            "x-lakecat-principal",
            None,
        )
    } else if let Some(did) = agent_did {
        (
            Principal::new(did, PrincipalKind::Agent)?,
            "x-lakecat-agent-did",
            None,
        )
    } else if let Some(authorization) = authorization {
        if let Some(token) = authorization.strip_prefix("Bearer ") {
            (
                Principal::new(
                    format!("bearer:{}", content_hash_bytes(token.as_bytes())),
                    PrincipalKind::Service,
                )?,
                "authorization",
                Some(content_hash_bytes(token.as_bytes())),
            )
        } else {
            return Err(LakeCatError::InvalidArgument(
                "unsupported Authorization scheme; use Bearer".to_string(),
            )
            .into());
        }
    } else {
        (Principal::anonymous(), "anonymous", None)
    };

    let envelope = json!({
        "type": "lakecat.request-identity.v1",
        "principal": principal,
        "source": source,
        "agent-did": agent_did,
        "typedid": typedid,
        "typedid-proof-sha256": typedid_proof.map(|value| content_hash_bytes(value.as_bytes())),
        "agent-delegation-sha256": delegation.map(|value| content_hash_bytes(value.as_bytes())),
        "agent-summary-signature-sha256": signed_summary
            .map(|value| content_hash_bytes(value.as_bytes())),
        "bearer-token-sha256": bearer_token_sha256,
        "attestation-state": "unverified",
        "raw-secret-material": false,
    });

    Ok(RequestIdentity {
        principal,
        envelope,
    })
}

async fn authorize(
    state: &LakeCatState,
    identity: RequestIdentity,
    action: CatalogAction,
    table: Option<TableIdent>,
) -> Result<AuthorizationReceipt, LakeCatHttpError> {
    let policy_bindings = if let Some(table) = table.as_ref() {
        state.store.policy_bindings_for_table(table).await?
    } else {
        Vec::new()
    };
    let receipt = state
        .governance
        .authorize(AuthorizationRequest {
            principal: identity.principal,
            action,
            table,
            context: json!({
                "warehouse": state.warehouse.as_str(),
                "request-identity": identity.envelope,
                "policy-bindings": policy_bindings
                    .iter()
                    .map(policy_binding_response)
                    .collect::<Vec<_>>(),
            }),
        })
        .await?;
    if receipt.allowed {
        Ok(receipt)
    } else {
        Err(LakeCatError::Conflict("authorization denied".to_string()).into())
    }
}

async fn authorize_table_create(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableCreateCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableCreate,
        Some(table.clone()),
    )
    .await?;
    Ok(TableCreateCapability::from_receipt(receipt, table)?)
}

async fn authorize_catalog_config(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<CatalogConfigCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::CatalogConfig, None).await?;
    Ok(CatalogConfigCapability::from_receipt(receipt)?)
}

async fn authorize_namespace_create(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceCreateCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceCreate, None).await?;
    Ok(NamespaceCreateCapability::from_receipt(receipt)?)
}

async fn authorize_namespace_list(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<NamespaceListCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::NamespaceList, None).await?;
    Ok(NamespaceListCapability::from_receipt(receipt)?)
}

async fn authorize_table_load(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableLoadCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableLoad,
        Some(table.clone()),
    )
    .await?;
    Ok(TableLoadCapability::from_receipt(receipt, table)?)
}

async fn authorize_table_commit(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableCommitCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableCommit,
        Some(table.clone()),
    )
    .await?;
    Ok(TableCommitCapability::from_receipt(receipt, table)?)
}

async fn authorize_table_drop(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableDropCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableDrop,
        Some(table.clone()),
    )
    .await?;
    Ok(TableDropCapability::from_receipt(receipt, table)?)
}

async fn authorize_table_restore(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableRestoreCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TableRestore,
        Some(table.clone()),
    )
    .await?;
    Ok(TableRestoreCapability::from_receipt(receipt, table)?)
}

async fn authorize_table_scan(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<TableScanCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::TablePlanScan,
        Some(table.clone()),
    )
    .await?;
    Ok(TableScanCapability::from_receipt(receipt, table)?)
}

async fn authorize_credentials_vend(
    state: &LakeCatState,
    identity: RequestIdentity,
    table: TableIdent,
) -> Result<CredentialsVendCapability, LakeCatHttpError> {
    let receipt = authorize(
        state,
        identity,
        CatalogAction::CredentialsVend,
        Some(table.clone()),
    )
    .await?;
    Ok(CredentialsVendCapability::from_receipt(receipt, table)?)
}

async fn authorize_storage_profile_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<StorageProfileManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::StorageProfileManage, None).await?;
    Ok(StorageProfileManageCapability::from_receipt(receipt)?)
}

async fn authorize_policy_manage(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<PolicyManageCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::PolicyManage, None).await?;
    Ok(PolicyManageCapability::from_receipt(receipt)?)
}

async fn authorize_graph_read(
    state: &LakeCatState,
    identity: RequestIdentity,
) -> Result<GraphReadCapability, LakeCatHttpError> {
    let receipt = authorize(state, identity, CatalogAction::GraphRead, None).await?;
    Ok(GraphReadCapability::from_receipt(receipt)?)
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
    use lakecat_lineage::LineageReceipt;
    use lakecat_store::{MemoryCatalogStore, OutboxEvent};
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    #[derive(Debug, Default)]
    struct RecordingGovernance {
        principals: Mutex<Vec<Principal>>,
        contexts: Mutex<Vec<serde_json::Value>>,
    }

    #[derive(Debug, Default)]
    struct RecordingGraph {
        events: Mutex<Vec<GraphEvent>>,
    }

    #[async_trait]
    impl CatalogGraphSink for RecordingGraph {
        async fn emit(&self, event: GraphEvent) -> lakecat_core::LakeCatResult<()> {
            self.events.lock().await.push(event);
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct RecordingLineage {
        events: Mutex<Vec<LineageEvent>>,
    }

    #[async_trait]
    impl LineageSink for RecordingLineage {
        async fn emit(&self, event: LineageEvent) -> lakecat_core::LakeCatResult<LineageReceipt> {
            self.events.lock().await.push(event);
            Ok(LineageReceipt {
                event_hash: "recorded".to_string(),
                open_lineage_hash: "recorded-openlineage".to_string(),
                sink: "recording".to_string(),
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingCredentialIssuer {
        requests: Mutex<Vec<CredentialIssuanceRequest>>,
    }

    #[async_trait]
    impl CredentialIssuer for RecordingCredentialIssuer {
        async fn issue(
            &self,
            request: CredentialIssuanceRequest,
        ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
            self.requests.lock().await.push(request.clone());
            if request.profile.issuance_mode == CredentialIssuanceMode::ShortLivedSecretRef {
                return Ok(vec![StorageCredential {
                    prefix: request.profile.location_prefix.clone(),
                    config: vec![
                        ConfigEntry::new("lakecat.storage-profile-id", request.profile.profile_id),
                        ConfigEntry::new("lakecat.credential-kind", "mock-short-lived"),
                        ConfigEntry::new(
                            "lakecat.authorization-principal",
                            request.authorization_receipt.principal.subject,
                        ),
                        ConfigEntry::new("aws.session-token", "temporary-test-token"),
                    ],
                }]);
            }
            Ok(public_storage_credentials_for_profile(&request.profile))
        }
    }

    #[cfg(feature = "typesec-local")]
    #[derive(Debug)]
    struct AllowCredentialIssuePolicy {
        subject: String,
        resource: String,
    }

    #[cfg(feature = "typesec-local")]
    impl typesec::PolicyEngine for AllowCredentialIssuePolicy {
        fn check(
            &self,
            subject: &typesec::SubjectId,
            action: &str,
            resource: &typesec::ResourceId,
        ) -> typesec::PolicyResult {
            if subject.as_str() == self.subject
                && action == "credentials.issue"
                && resource.as_str() == self.resource
            {
                typesec::PolicyResult::Allow
            } else {
                typesec::PolicyResult::Deny("not granted".to_string())
            }
        }
    }

    #[derive(Debug, Default)]
    struct RecordingOutboxStore {
        events: Mutex<Vec<OutboxEvent>>,
        delivered: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl CatalogStore for RecordingOutboxStore {
        async fn create_namespace(
            &self,
            _warehouse: &WarehouseName,
            _namespace: Namespace,
        ) -> lakecat_core::LakeCatResult<()> {
            Err(LakeCatError::NotSupported(
                "recording store does not create namespaces".to_string(),
            ))
        }

        async fn list_namespaces(
            &self,
            _warehouse: &WarehouseName,
        ) -> lakecat_core::LakeCatResult<Vec<Namespace>> {
            Err(LakeCatError::NotSupported(
                "recording store does not list namespaces".to_string(),
            ))
        }

        async fn list_tables(
            &self,
            _warehouse: &WarehouseName,
        ) -> lakecat_core::LakeCatResult<Vec<TableRecord>> {
            Err(LakeCatError::NotSupported(
                "recording store does not list tables".to_string(),
            ))
        }

        async fn create_table(
            &self,
            _table: TableRecord,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not create tables".to_string(),
            ))
        }

        async fn load_table(
            &self,
            _ident: &TableIdent,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not load tables".to_string(),
            ))
        }

        async fn commit_table(
            &self,
            _ident: &TableIdent,
            _commit: TableCommit,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not commit tables".to_string(),
            ))
        }

        async fn table_commit_records(
            &self,
            _ident: &TableIdent,
            _start_version: u64,
            _end_version: Option<u64>,
        ) -> lakecat_core::LakeCatResult<Vec<lakecat_store::TableCommitRecord>> {
            Err(LakeCatError::NotSupported(
                "recording store does not list table commits".to_string(),
            ))
        }

        async fn soft_delete_table(
            &self,
            _ident: &TableIdent,
            _principal: Principal,
            _authorization_receipt: Option<serde_json::Value>,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not delete tables".to_string(),
            ))
        }

        async fn restore_table(
            &self,
            _ident: &TableIdent,
            _principal: Principal,
            _authorization_receipt: Option<serde_json::Value>,
        ) -> lakecat_core::LakeCatResult<TableRecord> {
            Err(LakeCatError::NotSupported(
                "recording store does not restore tables".to_string(),
            ))
        }

        async fn pending_outbox_events(
            &self,
            sink: Option<&str>,
            limit: usize,
        ) -> lakecat_core::LakeCatResult<Vec<OutboxEvent>> {
            let events = self.events.lock().await;
            Ok(events
                .iter()
                .filter(|event| sink.is_none_or(|sink| event.sink == sink))
                .take(limit)
                .cloned()
                .collect())
        }

        async fn mark_outbox_delivered(
            &self,
            event_ids: &[String],
        ) -> lakecat_core::LakeCatResult<usize> {
            self.delivered.lock().await.extend_from_slice(event_ids);
            Ok(event_ids.len())
        }
    }

    #[async_trait]
    impl GovernanceEngine for RecordingGovernance {
        async fn authorize(
            &self,
            request: AuthorizationRequest,
        ) -> lakecat_core::LakeCatResult<lakecat_security::AuthorizationReceipt> {
            self.principals.lock().await.push(request.principal.clone());
            self.contexts.lock().await.push(request.context.clone());
            Ok(lakecat_security::AuthorizationReceipt {
                principal: request.principal,
                action: request.action,
                table: request.table,
                allowed: true,
                engine: "recording".to_string(),
                policy_hash: None,
                context: request.context,
                checked_at: chrono::Utc::now(),
            })
        }
    }

    #[test]
    fn request_identity_hashes_typedid_envelope_material() {
        let mut headers = HeaderMap::new();
        headers.insert("x-lakecat-agent-did", "did:example:agent".parse().unwrap());
        headers.insert("x-lakecat-typedid-proof", "signed-proof".parse().unwrap());
        headers.insert(
            "x-lakecat-agent-delegation",
            "delegation-token".parse().unwrap(),
        );
        headers.insert(
            "x-lakecat-agent-summary-signature",
            "summary-secret".parse().unwrap(),
        );

        let identity = request_identity(&headers).expect("identity should parse");

        assert_eq!(identity.principal.subject, "did:example:agent");
        assert_eq!(identity.principal.kind, PrincipalKind::Agent);
        assert_eq!(
            identity.envelope["typedid-proof-sha256"],
            serde_json::json!(content_hash_bytes("signed-proof".as_bytes()))
        );
        assert_eq!(
            identity.envelope["agent-delegation-sha256"],
            serde_json::json!(content_hash_bytes("delegation-token".as_bytes()))
        );
        assert_eq!(
            identity.envelope["agent-summary-signature-sha256"],
            serde_json::json!(content_hash_bytes("summary-secret".as_bytes()))
        );
        assert_eq!(
            identity.envelope["raw-secret-material"],
            serde_json::json!(false)
        );
        let envelope = identity.envelope.to_string();
        assert!(!envelope.contains("signed-proof"));
        assert!(!envelope.contains("delegation-token"));
        assert!(!envelope.contains("summary-secret"));
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
        drop(principals);
        let contexts = governance.contexts.lock().await;
        assert_eq!(
            contexts[0]["request-identity"]["source"],
            serde_json::json!("x-lakecat-principal")
        );
        assert_eq!(
            contexts[0]["request-identity"]["principal"]["subject"],
            serde_json::json!("alice@example.com")
        );
    }

    #[tokio::test]
    async fn outbox_drain_projects_table_events_to_sinks() {
        let table = TableIdent::new(
            WarehouseName::new("local").unwrap(),
            "default".parse::<Namespace>().unwrap(),
            TableName::new("events").unwrap(),
        );
        let principal = Principal {
            subject: "agent:writer".to_string(),
            kind: PrincipalKind::Agent,
        };
        let store = Arc::new(RecordingOutboxStore {
            events: Mutex::new(vec![OutboxEvent {
                event_id: "evt-1".to_string(),
                sink: "lakecat.lineage-and-graph".to_string(),
                event_type: "table.created".to_string(),
                payload: json!({
                    "audit-event-id": "audit-1",
                    "event-type": "table.created",
                    "table": table,
                    "payload": {
                        "authorization-receipt": {
                            "principal": principal,
                            "action": "table-create",
                            "allowed": true,
                            "engine": "test",
                            "policy_hash": null,
                            "checked_at": chrono::Utc::now(),
                        },
                        "metadata-location": "file:///tmp/events/metadata/00000.json",
                    }
                }),
                created_at: chrono::Utc::now(),
                delivered_at: None,
            }]),
            delivered: Mutex::default(),
        });
        let graph = Arc::new(RecordingGraph::default());
        let lineage = Arc::new(RecordingLineage::default());
        let state = LakeCatState::new(WarehouseName::new("local").unwrap(), store.clone())
            .with_integrations(
                default_sail_engine(),
                AllowAllGovernanceEngine::new(),
                graph.clone(),
                lineage.clone(),
            );

        assert_eq!(drain_outbox_once(&state, 10).await.unwrap(), 1);

        let graph_events = graph.events.lock().await;
        assert_eq!(graph_events.len(), 1);
        assert_eq!(graph_events[0].action, GraphAction::Created);
        let lineage_events = lineage.events.lock().await;
        assert_eq!(lineage_events.len(), 1);
        assert_eq!(lineage_events[0].event_type, LineageEventType::TableCreated);
        assert_eq!(
            store.delivered.lock().await.as_slice(),
            &["evt-1".to_string()]
        );
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

    #[tokio::test]
    async fn load_credentials_returns_scoped_local_file_profile_without_raw_secrets() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let credentials = body["storage-credentials"].as_array().unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(
            credentials[0]["prefix"],
            serde_json::json!("file:///tmp/events")
        );
        let config = credentials[0]["config"].as_array().unwrap();
        assert!(config.iter().any(|entry| {
            entry["key"] == "lakecat.credential-mode" && entry["value"] == "local-file-no-secret"
        }));
        assert!(!config.iter().any(|entry| {
            entry["key"]
                .as_str()
                .is_some_and(|key| key.contains("secret") || key.contains("token"))
        }));
    }

    #[tokio::test]
    async fn load_credentials_returns_empty_for_remote_profile_until_issuance_exists() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"s3://lakecat-demo/events","metadata-location":"s3://lakecat-demo/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["storage-credentials"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn delete_table_soft_deletes_from_catalog_reads() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let delete = Request::builder()
            .method(Method::DELETE)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(delete).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let delete_again = Request::builder()
            .method(Method::DELETE)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(delete_again).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn restore_table_reopens_soft_deleted_catalog_reads() {
        let app = test_app();
        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let delete = Request::builder()
            .method(Method::DELETE)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(delete).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let restore = Request::builder()
            .method(Method::POST)
            .uri("/management/v1/warehouses/local/namespaces/default/tables/events/restore")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(restore).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["identifier"]["name"], serde_json::json!("events"));

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn management_storage_profile_overrides_inferred_credentials_by_prefix() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/local-events")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "file:///tmp/events",
                    "provider": "file",
                    "issuance-mode": "local-file-no-secret",
                    "public-config": {
                        "lakecat.endpoint": "local"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["profile-id"], serde_json::json!("local-events"));

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses/local/storage-profiles")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["storage-profiles"].as_array().unwrap().len(), 1);

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events/tenant-a","metadata-location":"file:///tmp/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let credentials = body["storage-credentials"].as_array().unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(
            credentials[0]["prefix"],
            serde_json::json!("file:///tmp/events")
        );
        let config = credentials[0]["config"].as_array().unwrap();
        assert!(config.iter().any(|entry| {
            entry["key"] == "lakecat.storage-profile-id" && entry["value"] == "local-events"
        }));
        assert!(
            config
                .iter()
                .any(|entry| { entry["key"] == "lakecat.endpoint" && entry["value"] == "local" })
        );
    }

    #[tokio::test]
    async fn remote_storage_profile_accepts_secret_ref_without_vending_raw_secrets() {
        let app = test_app();
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/s3-events")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "s3://lakecat-demo/events",
                    "provider": "s3",
                    "issuance-mode": "short-lived-secret-ref",
                    "secret-ref": "typesec://lakecat/local/s3-events",
                    "public-config": {
                        "lakecat.region": "us-west-2"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["secret-ref"],
            serde_json::json!("typesec://lakecat/local/s3-events")
        );

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"s3://lakecat-demo/events/tenant-a","metadata-location":"s3://lakecat-demo/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["storage-credentials"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn credential_issuer_vends_short_lived_credentials_for_secret_ref_profile() {
        let issuer = Arc::new(RecordingCredentialIssuer::default());
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        )
        .with_credential_issuer(issuer.clone()));
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/s3-events")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "s3://lakecat-demo/events",
                    "provider": "s3",
                    "issuance-mode": "short-lived-secret-ref",
                    "secret-ref": "typesec://lakecat/local/s3-events"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"s3://lakecat-demo/events/tenant-a","metadata-location":"s3://lakecat-demo/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let credentials = body["storage-credentials"].as_array().unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(
            credentials[0]["prefix"],
            serde_json::json!("s3://lakecat-demo/events")
        );
        let config = credentials[0]["config"].as_array().unwrap();
        assert!(config.iter().any(|entry| {
            entry["key"] == "lakecat.credential-kind" && entry["value"] == "mock-short-lived"
        }));
        assert!(
            !config
                .iter()
                .any(|entry| { entry["value"] == "typesec://lakecat/local/s3-events" })
        );

        let requests = issuer.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].profile.secret_ref.as_deref(),
            Some("typesec://lakecat/local/s3-events")
        );
        assert_eq!(
            requests[0].authorization_receipt.principal.subject,
            "did:example:agent"
        );
    }

    #[cfg(feature = "typesec-local")]
    #[tokio::test]
    async fn typesec_credential_issuer_gates_secret_ref_resolution() {
        use std::collections::BTreeMap;

        use crate::typesec_credential_issuer::{
            StaticSecretRefCredentialResolver, TypeSecCredentialIssuer,
        };

        let issuer = TypeSecCredentialIssuer::new(
            Arc::new(AllowCredentialIssuePolicy {
                subject: "did:example:agent".to_string(),
                resource: "typesec://lakecat/local/s3-events".to_string(),
            }),
            StaticSecretRefCredentialResolver::new(BTreeMap::from([(
                "typesec://lakecat/local/s3-events".to_string(),
                vec![
                    ConfigEntry::new("lakecat.credential-kind", "typesec-short-lived"),
                    ConfigEntry::new("aws.session-token", "temporary-typesec-token"),
                ],
            )])),
        );
        let app = app(LakeCatState::new(
            WarehouseName::new("local").unwrap(),
            MemoryCatalogStore::new(),
        )
        .with_credential_issuer(issuer));
        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/storage-profiles/s3-events")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "location-prefix": "s3://lakecat-demo/events",
                    "provider": "s3",
                    "issuance-mode": "short-lived-secret-ref",
                    "secret-ref": "typesec://lakecat/local/s3-events"
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"s3://lakecat-demo/events/tenant-a","metadata-location":"s3://lakecat-demo/events/tenant-a/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let credentials = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(credentials).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let credentials = body["storage-credentials"].as_array().unwrap();
        assert_eq!(credentials.len(), 1);
        let config = credentials[0]["config"].as_array().unwrap();
        assert!(config.iter().any(|entry| {
            entry["key"] == "lakecat.credential-kind" && entry["value"] == "typesec-short-lived"
        }));

        let denied = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events/credentials")
            .header("x-lakecat-agent-did", "did:example:other")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(denied).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn policy_bindings_are_governed_and_attached_to_table_authorization_context() {
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

        let upsert = Request::builder()
            .method(Method::PUT)
            .uri("/management/v1/warehouses/local/policies/agent-read")
            .header("content-type", "application/json")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::from(
                serde_json::json!({
                    "namespace": ["default"],
                    "table": "events",
                    "enforced": true,
                    "odrl": {
                        "uid": "policy:agent-read",
                        "permission": [{
                            "action": "read",
                            "constraint": [{
                                "leftOperand": "purpose",
                                "operator": "eq",
                                "rightOperand": "resilience-demo"
                            }]
                        }]
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(upsert).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let list = Request::builder()
            .method(Method::GET)
            .uri("/management/v1/warehouses/local/policies")
            .header("x-lakecat-principal", "operator@example.com")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(list).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["policies"].as_array().unwrap().len(), 1);

        let create = Request::builder()
            .method(Method::POST)
            .uri("/catalog/v1/namespaces/default/tables")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"events","location":"file:///tmp/events","metadata-location":"file:///tmp/events/metadata/00000.json","metadata":{"format-version":3,"current-schema-id":1,"schemas":[{"schema-id":1,"fields":[{"id":1,"name":"event_id","type":"string","required":true}]}]}}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let load = Request::builder()
            .method(Method::GET)
            .uri("/catalog/v1/namespaces/default/tables/events")
            .header("x-lakecat-agent-did", "did:example:agent")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(load).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let contexts = governance.contexts.lock().await;
        let load_context = contexts
            .iter()
            .find(|context| {
                context["policy-bindings"]
                    .as_array()
                    .is_some_and(|bindings| !bindings.is_empty())
            })
            .expect("table authorization should include active policy bindings");
        assert_eq!(
            load_context["policy-bindings"][0]["policy-id"],
            serde_json::json!("agent-read")
        );
        assert_eq!(
            load_context["policy-bindings"][0]["odrl"]["uid"],
            serde_json::json!("policy:agent-read")
        );
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
