#![allow(unused_imports)]
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use http::{HeaderValue, Method, Request};
use lakecat_api::{
    CatalogConfigResponse, CommitTableRequest, CommitTableResponse, ConfigEntry,
    CreateNamespaceRequest, CreateTableRequest, FetchScanTasksRequest as ApiFetchScanTasksRequest,
    FetchScanTasksResponse, LAKECAT_COMPATIBILITY_KEY, LAKECAT_COMPATIBILITY_VALUE,
    LAKECAT_FORMAT_BASELINE_KEY, LAKECAT_FORMAT_BASELINE_VALUE, LAKECAT_FORMAT_V4_BRIDGE_KEY,
    LAKECAT_FORMAT_V4_BRIDGE_VALUE, LAKECAT_FORMAT_V4_KEY, LAKECAT_FORMAT_V4_TYPED_SAIL_KEY,
    LAKECAT_FORMAT_V4_TYPED_SAIL_VALUE, LAKECAT_FORMAT_V4_VALUE, LineageDrainEventSummary,
    LineageDrainResponse, ListNamespacesResponse, ListPolicyBindingsResponse, ListProjectsResponse,
    ListServersResponse, ListStorageProfilesResponse, ListTableCommitRecordsResponse,
    ListViewVersionReceiptChainsResponse, ListViewVersionReceiptsResponse, ListViewsResponse,
    ListWarehousesResponse, LoadCredentialsResponse, LoadTableResponse, NamespaceResponse,
    PlanTableScanRequest, PlanTableScanResponse, PolicyBindingResponse, ProjectResponse,
    ServerResponse, StorageCredential, StorageProfileResponse, TableCommitRecordResponse,
    TableIdentifier, UpsertPolicyBindingRequest, UpsertProjectRequest, UpsertServerRequest,
    UpsertStorageProfileRequest, UpsertViewRequest, UpsertWarehouseRequest, ViewColumnResponse,
    ViewResponse, ViewVersionReceiptChainResponse, ViewVersionReceiptResponse, WarehouseResponse,
};
#[cfg(not(feature = "sail-local"))]
use lakecat_core::sail::DeferredSailCatalogEngine;
#[cfg(not(feature = "sail-local"))]
use lakecat_core::sail::FetchScanTasksRequest as SailFetchScanTasksRequest;
#[cfg(not(feature = "sail-local"))]
use lakecat_core::sail::ScanPlanningRequest;
use lakecat_core::sail::{CommitPreparationRequest, SailCatalogEngine};
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, PrincipalKind, TableIdent, TableName,
    WarehouseName, content_hash_bytes, content_hash_json,
};
use lakecat_graph::GraphNodeLabel;
use lakecat_graph::{CatalogGraphSink, GraphAction, GraphEvent, NoopCatalogGraphSink};
use lakecat_lineage::{
    HashOnlyLineageSink, LineageEvent, LineageEventType, LineageReceipt, LineageSink,
};
use lakecat_querygraph::{
    QueryGraphBootstrap, QueryGraphTenantProjection, QueryGraphViewReceiptEvidence,
};
#[cfg(feature = "sail-local")]
use lakecat_sail::catalog_provider::{
    LakeCatCatalogProvider, ProviderFetchScanTasksRequest, ProviderScanPlanningRequest,
};
use lakecat_security::{
    AllowAllGovernanceEngine, AuthorizationReceipt, AuthorizationRequest, CatalogAction,
    CatalogConfigCapability, CredentialsVendCapability, GovernanceEngine, GraphReadCapability,
    LineageReadCapability, NamespaceCreateCapability, NamespaceDropCapability,
    NamespaceListCapability, NamespaceLoadCapability, PolicyManageCapability,
    ProjectManageCapability, ReadRestriction, ServerManageCapability,
    StorageProfileManageCapability, TableCommitCapability, TableCreateCapability,
    TableDropCapability, TableLoadCapability, TableRestoreCapability, TableScanCapability,
    ViewDropCapability, ViewLoadCapability, ViewManageCapability, WarehouseManageCapability,
};
use lakecat_store::MemoryCatalogStore;
use lakecat_store::{
    CatalogAuditEvent, CatalogStore, CredentialIssuanceMode, OutboxEvent, PolicyBinding,
    ProjectRecord, ServerRecord, StorageProfile, StorageProvider, TableCommit, TableCommitRecord,
    TableRecord, ViewColumnRecord, ViewRecord, ViewVersionOperation, ViewVersionReceipt,
    WarehouseRecord, table_ident,
};
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, PutMode, PutPayload};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tower::ServiceExt;
use url::Url;

use crate::*;

pub(crate) fn is_full_sha256_hash(value: &str) -> bool {
    is_full_sha256_digest_evidence(value)
}

#[derive(Debug, Default)]
pub(crate) struct RecordingGovernance {
    pub(crate) principals: Mutex<Vec<Principal>>,
    pub(crate) contexts: Mutex<Vec<serde_json::Value>>,
    pub(crate) actions: Mutex<Vec<CatalogAction>>,
}

#[derive(Debug)]
pub(crate) struct StaticTypeDidVerifier {
    pub(crate) verification: TypeDidVerification,
}

#[async_trait]
impl TypeDidVerifier for StaticTypeDidVerifier {
    async fn verify(
        &self,
        _envelope_json: &str,
    ) -> lakecat_core::LakeCatResult<TypeDidVerification> {
        Ok(self.verification.clone())
    }
}

#[derive(Debug)]
pub(crate) struct LeakingTypeDidVerifier {
    pub(crate) err: LakeCatError,
}

#[async_trait]
impl TypeDidVerifier for LeakingTypeDidVerifier {
    async fn verify(
        &self,
        _envelope_json: &str,
    ) -> lakecat_core::LakeCatResult<TypeDidVerification> {
        Err(match &self.err {
            LakeCatError::InvalidArgument(message) => {
                LakeCatError::InvalidArgument(message.clone())
            }
            LakeCatError::Conflict(message) => LakeCatError::Conflict(message.clone()),
            LakeCatError::NotSupported(message) => LakeCatError::NotSupported(message.clone()),
            LakeCatError::Internal(message) => LakeCatError::Internal(message.clone()),
            LakeCatError::NotFound { object, name } => LakeCatError::NotFound {
                object,
                name: name.clone(),
            },
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecordingGraph {
    pub(crate) events: Mutex<Vec<GraphEvent>>,
}

#[async_trait]
impl CatalogGraphSink for RecordingGraph {
    async fn emit(&self, event: GraphEvent) -> lakecat_core::LakeCatResult<()> {
        self.events.lock().await.push(event);
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct FailingGraph {
    pub(crate) events: Mutex<Vec<GraphEvent>>,
}

#[async_trait]
impl CatalogGraphSink for FailingGraph {
    async fn emit(&self, event: GraphEvent) -> lakecat_core::LakeCatResult<()> {
        self.events.lock().await.push(event);
        Err(LakeCatError::Internal(
            "intentional graph projection failure".to_string(),
        ))
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecordingLineage {
    pub(crate) events: Mutex<Vec<LineageEvent>>,
}

#[async_trait]
impl LineageSink for RecordingLineage {
    async fn emit(&self, event: LineageEvent) -> lakecat_core::LakeCatResult<LineageReceipt> {
        let event_hash = content_hash_json(&json!({
            "sink": "recording",
            "event": event,
        }))?;
        let open_lineage_hash = content_hash_json(&json!({
            "sink": "recording-openlineage",
            "event-hash": event_hash,
        }))?;
        self.events.lock().await.push(event);
        Ok(LineageReceipt {
            event_hash,
            open_lineage_hash,
            sink: "recording".to_string(),
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct FailingLineage {
    pub(crate) events: Mutex<Vec<LineageEvent>>,
}

#[async_trait]
impl LineageSink for FailingLineage {
    async fn emit(&self, event: LineageEvent) -> lakecat_core::LakeCatResult<LineageReceipt> {
        self.events.lock().await.push(event);
        Err(LakeCatError::Internal(
            "intentional lineage projection failure".to_string(),
        ))
    }
}

#[derive(Debug)]
pub(crate) struct FailingLineageAfter {
    pub(crate) events: Mutex<Vec<LineageEvent>>,
    pub(crate) fail_after: usize,
}

#[async_trait]
impl LineageSink for FailingLineageAfter {
    async fn emit(&self, event: LineageEvent) -> lakecat_core::LakeCatResult<LineageReceipt> {
        let mut events = self.events.lock().await;
        let event_index = events.len() + 1;
        let event_hash = content_hash_json(&json!({
            "sink": "recording",
            "event-index": event_index,
            "event": event,
        }))?;
        let open_lineage_hash = content_hash_json(&json!({
            "sink": "recording-openlineage",
            "event-hash": event_hash,
        }))?;
        events.push(event);
        if events.len() > self.fail_after {
            return Err(LakeCatError::Internal(
                "intentional later lineage projection failure".to_string(),
            ));
        }
        Ok(LineageReceipt {
            event_hash,
            open_lineage_hash,
            sink: "recording".to_string(),
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecordingCredentialIssuer {
    pub(crate) requests: Mutex<Vec<CredentialIssuanceRequest>>,
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

#[derive(Debug, Default)]
pub(crate) struct DuplicateTtlCredentialIssuer {
    pub(crate) requests: Mutex<Vec<CredentialIssuanceRequest>>,
}

#[async_trait]
impl CredentialIssuer for DuplicateTtlCredentialIssuer {
    async fn issue(
        &self,
        request: CredentialIssuanceRequest,
    ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
        self.requests.lock().await.push(request.clone());
        Ok(vec![StorageCredential {
            prefix: request.profile.location_prefix.clone(),
            config: vec![
                ConfigEntry::new("lakecat.credential-kind", "duplicate-ttl-test"),
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "600"),
                ConfigEntry::new("aws.session-token", "temporary-test-token"),
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "120"),
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "not-a-number"),
            ],
        }])
    }
}

#[derive(Debug, Default)]
pub(crate) struct ShadowingCredentialEvidenceIssuer {
    pub(crate) requests: Mutex<Vec<CredentialIssuanceRequest>>,
}

#[async_trait]
impl CredentialIssuer for ShadowingCredentialEvidenceIssuer {
    async fn issue(
        &self,
        request: CredentialIssuanceRequest,
    ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
        self.requests.lock().await.push(request.clone());
        Ok(vec![StorageCredential {
            prefix: request.profile.location_prefix.clone(),
            config: vec![
                ConfigEntry::new("lakecat.storage-profile-id", "forged-profile"),
                ConfigEntry::new("lakecat.storage-provider", "gcs"),
                ConfigEntry::new("lakecat.credential-mode", "forged-mode"),
                ConfigEntry::new("lakecat.authorization-principal", "did:example:attacker"),
                ConfigEntry::new("lakecat.governed-read-required", "false"),
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "600"),
                ConfigEntry::new("lakecat.credential-kind", "shadow-test"),
                ConfigEntry::new("aws.session-token", "temporary-test-token"),
                ConfigEntry::new("lakecat.max-credential-ttl-seconds", "120"),
            ],
        }])
    }
}

#[derive(Debug, Default)]
pub(crate) struct BroadCredentialIssuer {
    pub(crate) requests: Mutex<Vec<CredentialIssuanceRequest>>,
}

#[async_trait]
impl CredentialIssuer for BroadCredentialIssuer {
    async fn issue(
        &self,
        request: CredentialIssuanceRequest,
    ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
        self.requests.lock().await.push(request.clone());
        Ok(vec![StorageCredential {
            prefix: "s3://lakecat-demo".to_string(),
            config: vec![
                ConfigEntry::new("lakecat.credential-kind", "broad-test"),
                ConfigEntry::new("aws.session-token", "temporary-test-token"),
            ],
        }])
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecordingSailEngine {
    pub(crate) commit_prepare_count: Mutex<usize>,
}

#[async_trait]
impl SailCatalogEngine for RecordingSailEngine {
    async fn prepare_commit(
        &self,
        _request: lakecat_core::sail::CommitPreparationRequest,
    ) -> lakecat_core::LakeCatResult<lakecat_core::sail::CommitPlan> {
        *self.commit_prepare_count.lock().await += 1;
        Err(LakeCatError::Internal(
            "recording Sail engine should not prepare commit".to_string(),
        ))
    }

    async fn plan_scan(
        &self,
        _request: lakecat_core::sail::ScanPlanningRequest,
    ) -> lakecat_core::LakeCatResult<lakecat_core::sail::ScanPlan> {
        Err(LakeCatError::NotSupported(
            "recording Sail engine does not plan scans".to_string(),
        ))
    }

    async fn fetch_scan_tasks(
        &self,
        _request: lakecat_core::sail::FetchScanTasksRequest,
    ) -> lakecat_core::LakeCatResult<lakecat_core::sail::FetchScanTasksPlan> {
        Err(LakeCatError::NotSupported(
            "recording Sail engine does not fetch scan tasks".to_string(),
        ))
    }
}

#[derive(Debug, Default)]
pub(crate) struct CapturingSailEngine {
    pub(crate) last_scan: Mutex<Option<lakecat_core::sail::ScanPlanningRequest>>,
    pub(crate) last_fetch: Mutex<Option<lakecat_core::sail::FetchScanTasksRequest>>,
}

#[async_trait]
impl SailCatalogEngine for CapturingSailEngine {
    async fn prepare_commit(
        &self,
        _request: lakecat_core::sail::CommitPreparationRequest,
    ) -> lakecat_core::LakeCatResult<lakecat_core::sail::CommitPlan> {
        Err(LakeCatError::Internal(
            "capturing Sail engine should not prepare commit".to_string(),
        ))
    }

    async fn plan_scan(
        &self,
        request: lakecat_core::sail::ScanPlanningRequest,
    ) -> lakecat_core::LakeCatResult<lakecat_core::sail::ScanPlan> {
        *self.last_scan.lock().await = Some(request.clone());
        Ok(lakecat_core::sail::ScanPlan {
            planned_by: "capturing-sail".to_string(),
            snapshot_id: Some(42),
            scan_tasks: vec![serde_json::json!({
                "plan-task": "lakecat:plan:captured"
            })],
            residual_filter: Some(serde_json::json!({
                "projection": request.projection,
                "filters": request.filters
            })),
        })
    }

    async fn fetch_scan_tasks(
        &self,
        request: lakecat_core::sail::FetchScanTasksRequest,
    ) -> lakecat_core::LakeCatResult<lakecat_core::sail::FetchScanTasksPlan> {
        *self.last_fetch.lock().await = Some(request.clone());
        Ok(lakecat_core::sail::FetchScanTasksPlan {
            planned_by: "capturing-sail".to_string(),
            plan_task: request.plan_task,
            snapshot_id: Some(42),
            file_scan_tasks: vec![
                serde_json::json!({"file-path": "file:///tmp/events/data.parquet"}),
            ],
            delete_files: Vec::new(),
            plan_tasks: Vec::new(),
            residual_filter: Some(serde_json::json!({
                "required-projection": request.required_projection,
                "required-filters": request.required_filters
            })),
        })
    }
}

pub(crate) fn test_view_receipt(
    view_version: u64,
    previous_view_version: Option<u64>,
    previous_receipt_hash: Option<&str>,
    operation: &str,
    receipt_hash: &str,
) -> ViewVersionReceiptResponse {
    ViewVersionReceiptResponse {
        stable_id: "lakecat:view:local:default:events_view".to_string(),
        warehouse: "local".to_string(),
        namespace: vec!["default".to_string()],
        name: "events_view".to_string(),
        view_version,
        previous_view_version,
        previous_receipt_hash: previous_receipt_hash.map(str::to_string),
        operation: operation.to_string(),
        view_hash: format!("sha256:view-{view_version}-{operation}"),
        receipt_hash: receipt_hash.to_string(),
        principal_subject: "operator@example.com".to_string(),
        principal_kind: "human".to_string(),
        recorded_at: "2026-06-19T00:00:00Z".to_string(),
    }
}

#[cfg(feature = "typesec-local")]
#[derive(Debug)]
pub(crate) struct AllowCredentialIssuePolicy {
    pub(crate) subject: String,
    pub(crate) resource: String,
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

#[cfg(feature = "typesec-local")]
#[derive(Debug, Default)]
pub(crate) struct MockVaultSecretClient {
    pub(crate) requests: Mutex<Vec<(String, String, Option<String>)>>,
    pub(crate) response: Mutex<Option<serde_json::Value>>,
    pub(crate) error: Mutex<Option<String>>,
}

#[cfg(feature = "typesec-local")]
#[async_trait]
impl crate::typesec_credential_issuer::VaultSecretClient for MockVaultSecretClient {
    async fn read_secret(
        &self,
        url: &str,
        token: &str,
        namespace: Option<&str>,
    ) -> lakecat_core::LakeCatResult<serde_json::Value> {
        self.requests.lock().await.push((
            url.to_string(),
            token.to_string(),
            namespace.map(ToString::to_string),
        ));
        if let Some(error) = self.error.lock().await.clone() {
            return Err(LakeCatError::InvalidArgument(error));
        }
        self.response
            .lock()
            .await
            .clone()
            .ok_or_else(|| LakeCatError::InvalidArgument("mock Vault response missing".to_string()))
    }
}

#[cfg(feature = "typesec-local")]
pub(crate) fn production_secret_credential_request(secret_ref: &str) -> CredentialIssuanceRequest {
    let principal = Principal::new("did:example:agent", PrincipalKind::Agent).unwrap();
    let table = TableRecord::new(
        table_ident("local", "default", "events").unwrap(),
        "s3://lakecat-demo/events/tenant-a".to_string(),
        Some("s3://lakecat-demo/events/tenant-a/metadata/00000.json".to_string()),
        serde_json::json!({"format-version":3}),
        principal.clone(),
    );
    let profile = StorageProfile::new(
        "s3-events",
        WarehouseName::new("local").unwrap(),
        "s3://lakecat-demo/events",
        StorageProvider::S3,
        CredentialIssuanceMode::ShortLivedSecretRef,
        Some(secret_ref.to_string()),
        Default::default(),
    )
    .unwrap();
    CredentialIssuanceRequest {
        table,
        profile,
        authorization_receipt: AuthorizationReceipt {
            principal,
            action: CatalogAction::CredentialsVend,
            table: Some(table_ident("local", "default", "events").unwrap()),
            allowed: true,
            engine: "test".to_string(),
            policy_hash: None,
            context: serde_json::json!({}),
            checked_at: chrono::Utc::now(),
        },
        max_credential_ttl_seconds: None,
    }
}

#[cfg(feature = "typesec-local")]
#[derive(Debug)]
pub(crate) struct MockProductionSecretRefResolver {
    pub(crate) provider_label: &'static str,
    pub(crate) credential_prefix: Option<&'static str>,
    pub(crate) requests: Mutex<Vec<(String, Option<u64>)>>,
}

#[cfg(feature = "typesec-local")]
#[async_trait]
impl crate::typesec_credential_issuer::SecretRefCredentialResolver
    for MockProductionSecretRefResolver
{
    async fn resolve(
        &self,
        request: &CredentialIssuanceRequest,
    ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
        let secret_ref = request.profile.secret_ref.clone().ok_or_else(|| {
            LakeCatError::InvalidArgument("mock production resolver missing secret ref".to_string())
        })?;
        self.requests
            .lock()
            .await
            .push((secret_ref, request.max_credential_ttl_seconds));
        Ok(vec![StorageCredential {
            prefix: self
                .credential_prefix
                .unwrap_or(request.profile.location_prefix.as_str())
                .to_string(),
            config: vec![ConfigEntry::new(
                "lakecat.credential-kind",
                format!("{}-short-lived", self.provider_label),
            )],
        }])
    }
}

#[cfg(feature = "typesec-local")]
#[derive(Debug)]
pub(crate) struct FailingProductionSecretRefResolver {
    pub(crate) error: &'static str,
    pub(crate) requests: Mutex<Vec<(String, Option<u64>)>>,
}

#[cfg(feature = "typesec-local")]
#[async_trait]
impl crate::typesec_credential_issuer::SecretRefCredentialResolver
    for FailingProductionSecretRefResolver
{
    async fn resolve(
        &self,
        request: &CredentialIssuanceRequest,
    ) -> lakecat_core::LakeCatResult<Vec<StorageCredential>> {
        let secret_ref = request.profile.secret_ref.clone().ok_or_else(|| {
            LakeCatError::InvalidArgument("mock production resolver missing secret ref".to_string())
        })?;
        self.requests
            .lock()
            .await
            .push((secret_ref, request.max_credential_ttl_seconds));
        Err(LakeCatError::InvalidArgument(self.error.to_string()))
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecordingOutboxStore {
    pub(crate) events: Mutex<Vec<OutboxEvent>>,
    pub(crate) delivered: Mutex<Vec<String>>,
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

    async fn create_table(&self, _table: TableRecord) -> lakecat_core::LakeCatResult<TableRecord> {
        Err(LakeCatError::NotSupported(
            "recording store does not create tables".to_string(),
        ))
    }

    async fn load_table(&self, _ident: &TableIdent) -> lakecat_core::LakeCatResult<TableRecord> {
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
        let mut events = events
            .iter()
            .filter(|event| sink.is_none_or(|sink| event.sink == sink))
            .cloned()
            .collect::<Vec<_>>();
        events.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        events.truncate(limit);
        Ok(events)
    }

    async fn mark_outbox_delivered(
        &self,
        event_ids: &[String],
    ) -> lakecat_core::LakeCatResult<usize> {
        self.delivered.lock().await.extend_from_slice(event_ids);
        if event_ids
            .iter()
            .any(|event_id| event_id == "evt-partial-ack")
        {
            return Ok(event_ids.len().saturating_sub(1));
        }
        Ok(event_ids.len())
    }
}

#[cfg(feature = "sail-local")]
pub(crate) struct CasRaceStore {
    pub(crate) inner: Arc<dyn CatalogStore>,
    pub(crate) racing_metadata_location: String,
    pub(crate) raced: Mutex<bool>,
}

#[cfg(feature = "sail-local")]
impl CasRaceStore {
    fn new(inner: Arc<dyn CatalogStore>, racing_metadata_location: String) -> Arc<Self> {
        Arc::new(Self {
            inner,
            racing_metadata_location,
            raced: Mutex::new(false),
        })
    }
}

#[cfg(feature = "sail-local")]
#[async_trait]
impl CatalogStore for CasRaceStore {
    async fn create_namespace(
        &self,
        warehouse: &WarehouseName,
        namespace: Namespace,
    ) -> lakecat_core::LakeCatResult<()> {
        self.inner.create_namespace(warehouse, namespace).await
    }

    async fn list_namespaces(
        &self,
        warehouse: &WarehouseName,
    ) -> lakecat_core::LakeCatResult<Vec<Namespace>> {
        self.inner.list_namespaces(warehouse).await
    }

    async fn list_tables(
        &self,
        warehouse: &WarehouseName,
    ) -> lakecat_core::LakeCatResult<Vec<TableRecord>> {
        self.inner.list_tables(warehouse).await
    }

    async fn create_table(&self, table: TableRecord) -> lakecat_core::LakeCatResult<TableRecord> {
        self.inner.create_table(table).await
    }

    async fn load_table(&self, ident: &TableIdent) -> lakecat_core::LakeCatResult<TableRecord> {
        self.inner.load_table(ident).await
    }

    async fn commit_table(
        &self,
        ident: &TableIdent,
        commit: TableCommit,
    ) -> lakecat_core::LakeCatResult<TableRecord> {
        let mut raced = self.raced.lock().await;
        if !*raced {
            *raced = true;
            self.inner
                .commit_table(
                    ident,
                    TableCommit {
                        requirements: Vec::new(),
                        updates: Vec::new(),
                        expected_previous_metadata_location: commit
                            .expected_previous_metadata_location
                            .clone(),
                        new_metadata_location: Some(self.racing_metadata_location.clone()),
                        new_metadata: None,
                        idempotency_key: None,
                        idempotency_request_hash: None,
                        principal: commit.principal.clone(),
                        authorization_receipt: commit.authorization_receipt.clone(),
                    },
                )
                .await?;
        }
        drop(raced);
        self.inner.commit_table(ident, commit).await
    }

    async fn table_commit_records(
        &self,
        ident: &TableIdent,
        start_version: u64,
        end_version: Option<u64>,
    ) -> lakecat_core::LakeCatResult<Vec<TableCommitRecord>> {
        self.inner
            .table_commit_records(ident, start_version, end_version)
            .await
    }

    async fn soft_delete_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<serde_json::Value>,
    ) -> lakecat_core::LakeCatResult<TableRecord> {
        self.inner
            .soft_delete_table(ident, principal, authorization_receipt)
            .await
    }

    async fn restore_table(
        &self,
        ident: &TableIdent,
        principal: Principal,
        authorization_receipt: Option<serde_json::Value>,
    ) -> lakecat_core::LakeCatResult<TableRecord> {
        self.inner
            .restore_table(ident, principal, authorization_receipt)
            .await
    }

    async fn upsert_storage_profile(
        &self,
        profile: StorageProfile,
    ) -> lakecat_core::LakeCatResult<StorageProfile> {
        self.inner.upsert_storage_profile(profile).await
    }

    async fn list_storage_profiles(
        &self,
        warehouse: &WarehouseName,
    ) -> lakecat_core::LakeCatResult<Vec<StorageProfile>> {
        self.inner.list_storage_profiles(warehouse).await
    }

    async fn storage_profile_for_table(
        &self,
        table: &TableRecord,
    ) -> lakecat_core::LakeCatResult<StorageProfile> {
        self.inner.storage_profile_for_table(table).await
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
        self.actions.lock().await.push(request.action.clone());
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

pub(crate) fn assert_single_config_value(config: &[ConfigEntry], key: &str, expected: &str) {
    let values = config
        .iter()
        .filter(|entry| entry.key == key)
        .map(|entry| entry.value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(values, vec![expected], "{key} must be canonical");
}

pub(crate) fn valid_lineage_summary_credential_event(event_id: &str) -> OutboxEvent {
    let payload = json!({
        "audit-event-id": format!("audit-{event_id}"),
        "event-type": "credentials.vend-attempted",
        "table": {
            "warehouse": "local",
            "namespace": ["default"],
            "name": "events"
        },
        "authorization-receipt": {
            "principal": {
                "subject": "human:operator",
                "kind": "human"
            },
            "action": "credentials-vend",
            "allowed": true,
            "engine": "lakecat-test",
            "policy_hash": null,
            "checked_at": chrono::Utc::now().to_rfc3339()
        },
        "storage-location": "file:///tmp/events",
        "storage-profile-id": "events-local",
        "mode": "local-file-no-secret",
        "storage-profile": {
            "profile-id": "events-local",
            "warehouse": "local",
            "provider": "file",
            "issuance-mode": "local-file-no-secret",
            "location-prefix-hash": content_hash_bytes(b"file:///tmp/events"),
            "secret-ref-present": false,
            "public-config": {}
        },
        "secret-ref-present": false,
        "credential-count": 1,
        "credential-response-evidence": [{
            "prefix-hash": content_hash_bytes(b"file:///tmp/events"),
            "issuer-config-hash": content_hash_bytes(b"issuer-config"),
            "issuer-config-entry-count": 1,
            "storage-profile-id": "events-local",
            "catalog-profile-id": "events-local",
            "storage-provider": "file",
            "credential-mode": "local-file-no-secret",
            "authorization-principal": "human:operator",
            "receipt-principal": "human:operator",
            "governed-read-required": "false"
        }]
    });
    OutboxEvent {
        event_id: event_id.to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "credentials.vend-attempted".to_string(),
        payload: json!({
            "audit-event-id": format!("audit-envelope-{event_id}"),
            "event-type": "credentials.vend-attempted",
            "table": {
                "warehouse": "local",
                "namespace": ["default"],
                "name": "events"
            },
            "payload": payload
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    }
}

pub(crate) fn valid_lineage_summary_storage_profile_upsert_event(event_id: &str) -> OutboxEvent {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    OutboxEvent {
        event_id: event_id.to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "storage-profile.upserted".to_string(),
        payload: json!({
            "audit-event-id": format!("audit-envelope-{event_id}"),
            "event-type": "storage-profile.upserted",
            "payload": {
                "authorization-receipt": {
                    "principal": principal,
                    "action": "storage-profile-manage",
                    "allowed": true,
                    "engine": "lakecat-test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now().to_rfc3339()
                },
                "warehouse": "local",
                "storage-profile": {
                    "profile-id": "s3-events",
                    "warehouse": "local",
                    "provider": "s3",
                    "issuance-mode": "short-lived-secret-ref",
                    "location-prefix-hash": content_hash_bytes(b"storage-profile-prefix"),
                    "secret-ref-present": true,
                    "secret-ref-provider": "typesec",
                    "secret-ref-hash": content_hash_bytes(b"storage-profile-secret-ref"),
                    "public-config": {
                        "region": "us-west-2"
                    }
                }
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    }
}

pub(crate) fn valid_lineage_summary_namespace_event(
    event_id: &str,
    event_type: &str,
) -> OutboxEvent {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let (action, payload) = match event_type {
        "namespace.listed" => (
            "namespace-list",
            json!({
                "event-type": "namespace.listed",
                "warehouse": "local",
                "namespace-count": 1,
                "namespace-paths": ["default"]
            }),
        ),
        "namespace.created" => (
            "namespace-create",
            json!({
                "event-type": "namespace.created",
                "warehouse": "local",
                "namespace": ["default"]
            }),
        ),
        "namespace.loaded" => (
            "namespace-load",
            json!({
                "event-type": "namespace.loaded",
                "warehouse": "local",
                "namespace": ["default"]
            }),
        ),
        "namespace.dropped" => (
            "namespace-drop",
            json!({
                "event-type": "namespace.dropped",
                "warehouse": "local",
                "namespace": ["default"]
            }),
        ),
        other => panic!("unsupported namespace event type: {other}"),
    };
    let mut payload = payload;
    payload["authorization-receipt"] = json!({
        "principal": principal,
        "action": action,
        "allowed": true,
        "engine": "lakecat-test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now().to_rfc3339()
    });
    OutboxEvent {
        event_id: event_id.to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: event_type.to_string(),
        payload: json!({
            "audit-event-id": format!("audit-envelope-{event_id}"),
            "event-type": event_type,
            "payload": payload
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    }
}

pub(crate) fn valid_lineage_summary_view_event(event_id: &str, event_type: &str) -> OutboxEvent {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let view = json!({
        "warehouse": "local",
        "namespace": ["default"],
        "name": "events_view",
        "view-version": 7,
        "sql": "select event_id from default.events",
        "dialect": "spark-sql",
        "schema-version": 1,
        "columns": [{
            "name": "event_id",
            "data-type": "string",
            "nullable": false,
            "comment": null
        }],
        "properties": {
            "owner": "querygraph"
        }
    });
    let (action, payload) = match event_type {
        "view.listed" => (
            "view-load",
            json!({
                "event-type": "view.listed",
                "warehouse": "local",
                "namespace": ["default"],
                "view-count": 1,
                "view-names": ["events_view"]
            }),
        ),
        "view.upserted" => (
            "view-manage",
            json!({
                "event-type": "view.upserted",
                "warehouse": "local",
                "namespace": ["default"],
                "view": view,
                "expected-view-version": 6
            }),
        ),
        "view.loaded" => (
            "view-load",
            json!({
                "event-type": "view.loaded",
                "warehouse": "local",
                "namespace": ["default"],
                "view": view
            }),
        ),
        "view.dropped" => (
            "view-drop",
            json!({
                "event-type": "view.dropped",
                "warehouse": "local",
                "namespace": ["default"],
                "view": view,
                "expected-view-version": 7
            }),
        ),
        other => panic!("unsupported view event type: {other}"),
    };
    let mut payload = payload;
    payload["authorization-receipt"] = json!({
        "principal": principal,
        "action": action,
        "allowed": true,
        "engine": "lakecat-test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now().to_rfc3339()
    });
    OutboxEvent {
        event_id: event_id.to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: event_type.to_string(),
        payload: json!({
            "audit-event-id": format!("audit-envelope-{event_id}"),
            "event-type": event_type,
            "payload": payload
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    }
}

pub(crate) fn valid_lineage_summary_management_upsert_event(
    event_id: &str,
    event_type: &str,
) -> OutboxEvent {
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let (action, payload) = match event_type {
        "policy-binding.upserted" => {
            let odrl = json!({
                "uid": "policy:agent-read",
                "permission": []
            });
            (
                "policy-manage",
                json!({
                    "event-type": "policy-binding.upserted",
                    "warehouse": "local",
                    "policy": {
                        "policy-id": "agent-read",
                        "warehouse": "local",
                        "namespace": ["default"],
                        "table": "events",
                        "enforced": true,
                        "odrl": odrl,
                        "odrl-hash": content_hash_json(&odrl).unwrap()
                    }
                }),
            )
        }
        "project.upserted" => (
            "project-manage",
            json!({
                "event-type": "project.upserted",
                "project-id": "analytics",
                "project-record": {
                    "project-id": "analytics",
                    "server-id": "primary",
                    "display-name": "Analytics",
                    "properties": {
                        "owner": "querygraph"
                    }
                }
            }),
        ),
        "server.upserted" => {
            let endpoint_url = "https://lakecat.example.com";
            (
                "server-manage",
                json!({
                    "event-type": "server.upserted",
                    "server-id": "primary",
                    "server-record": {
                        "server-id": "primary",
                        "display-name": "Primary",
                        "endpoint-url": endpoint_url,
                        "endpoint-url-hash": content_hash_json(&json!({
                            "endpoint-url": endpoint_url
                        })).unwrap(),
                        "properties": {
                            "region": "us-west"
                        }
                    }
                }),
            )
        }
        "warehouse.upserted" => {
            let storage_root = "file:///tmp/lakecat-analytics";
            (
                "warehouse-manage",
                json!({
                    "event-type": "warehouse.upserted",
                    "project-id": "analytics",
                    "warehouse": "local",
                    "warehouse-record": {
                        "warehouse": "local",
                        "project-id": "analytics",
                        "storage-root": storage_root,
                        "storage-root-hash": content_hash_json(&json!({
                            "storage-root": storage_root
                        })).unwrap(),
                        "properties": {
                            "owner": "querygraph"
                        }
                    }
                }),
            )
        }
        other => panic!("unsupported management upsert event type: {other}"),
    };
    let mut payload = payload;
    payload["authorization-receipt"] = json!({
        "principal": principal,
        "action": action,
        "allowed": true,
        "engine": "lakecat-test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now().to_rfc3339()
    });
    OutboxEvent {
        event_id: event_id.to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: event_type.to_string(),
        payload: json!({
            "audit-event-id": format!("audit-envelope-{event_id}"),
            "event-type": event_type,
            "payload": payload
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    }
}

pub(crate) fn valid_lineage_summary_management_list_event(
    event_id: &str,
    event_type: &str,
) -> OutboxEvent {
    let (action, payload) = match event_type {
        "policy-binding.listed" => (
            "policy-manage",
            json!({
                "event-type": "policy-binding.listed",
                "warehouse": "local",
                "policy-count": 1,
                "policy-ids": ["agent-read"]
            }),
        ),
        "project.listed" => (
            "project-manage",
            json!({
                "event-type": "project.listed",
                "project-count": 1,
                "project-ids": ["analytics"]
            }),
        ),
        "server.listed" => (
            "server-manage",
            json!({
                "event-type": "server.listed",
                "server-count": 1,
                "server-ids": ["primary"]
            }),
        ),
        "storage-profile.listed" => (
            "storage-profile-manage",
            json!({
                "event-type": "storage-profile.listed",
                "warehouse": "local",
                "storage-profile-count": 1,
                "storage-profile-ids": ["local-file"]
            }),
        ),
        "warehouse.listed" => (
            "warehouse-manage",
            json!({
                "event-type": "warehouse.listed",
                "warehouse-count": 1,
                "warehouse-names": ["local"]
            }),
        ),
        other => panic!("unsupported management list event type: {other}"),
    };
    let principal = Principal::new("agent:operator", PrincipalKind::Agent).unwrap();
    let mut payload = payload;
    payload["authorization-receipt"] = json!({
        "principal": principal,
        "action": action,
        "allowed": true,
        "engine": "lakecat-test",
        "policy_hash": null,
        "checked_at": chrono::Utc::now().to_rfc3339()
    });
    OutboxEvent {
        event_id: event_id.to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: event_type.to_string(),
        payload: json!({
            "audit-event-id": format!("audit-envelope-{event_id}"),
            "event-type": event_type,
            "payload": payload
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    }
}

pub(crate) fn valid_lineage_summary_catalog_config_event(event_id: &str) -> OutboxEvent {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    OutboxEvent {
        event_id: event_id.to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "catalog.config-read".to_string(),
        payload: json!({
            "audit-event-id": format!("audit-{event_id}"),
            "event-type": "catalog.config-read",
            "payload": {
                "authorization-receipt": {
                    "principal": principal,
                    "action": "catalog-config",
                    "allowed": true,
                    "engine": "lakecat-test",
                    "policy_hash": null,
                    "checked_at": chrono::Utc::now(),
                },
                "warehouse": "local",
                "defaults": catalog_config_defaults_json(),
                "overrides": [],
                "endpoints": catalog_config_endpoints_json(),
            }
        }),
        created_at: chrono::Utc::now(),
        delivered_at: None,
    }
}

pub(crate) fn test_app() -> Router {
    app(LakeCatState::new(
        WarehouseName::new("local").unwrap(),
        MemoryCatalogStore::new(),
    ))
}

pub(crate) fn catalog_config_defaults_json() -> serde_json::Value {
    serde_json::to_value(CatalogConfigResponse::default().defaults).unwrap()
}

pub(crate) fn catalog_config_endpoints_json() -> serde_json::Value {
    serde_json::to_value(CatalogConfigResponse::default().endpoints).unwrap()
}

pub(crate) fn valid_querygraph_bootstrap_payload(principal: Principal) -> serde_json::Value {
    let bundle_hash = content_hash_json(&json!({"querygraph-bootstrap": "bundle"})).unwrap();
    let graph_hash = content_hash_json(&json!({"querygraph-bootstrap": "graph"})).unwrap();
    let open_lineage_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "open-lineage"})).unwrap();
    let import_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "querygraph-import"})).unwrap();
    let table_hash = content_hash_json(&json!({"querygraph-bootstrap": "table"})).unwrap();
    let view_hash = content_hash_json(&json!({"querygraph-bootstrap": "view"})).unwrap();
    let receipt_hash = content_hash_json(&json!({"querygraph-bootstrap": "view-receipt"})).unwrap();
    let receipt_chain_hash =
        content_hash_json(&json!({"querygraph-bootstrap": "view-chain"})).unwrap();

    json!({
        "audit-event-id": "audit-bootstrap",
        "event-type": "querygraph.bootstrap",
        "payload": {
            "authorization-receipt": {
                "principal": principal,
                "action": "graph-read",
                "allowed": true,
                "engine": "test",
                "policy_hash": null,
                "checked_at": chrono::Utc::now(),
            },
            "warehouse": "local",
            "table-count": 1,
            "view-count": 1,
            "policy-binding-count": 1,
            "verified-tables": ["local.default.events"],
            "verified-views": ["lakecat:view:local:default:active_customers"],
            "bundle-hash": bundle_hash,
            "graph-hash": graph_hash,
            "open-lineage-hash": open_lineage_hash,
            "querygraph-import-hash": import_hash,
            "table-artifacts": [{
                "stable-id": "local.default.events",
                "croissant-hash": table_hash,
                "cdif-hash": table_hash,
                "osi-hash": table_hash,
                "odrl-hash": table_hash,
                "policy-bindings-hash": table_hash
            }],
            "view-artifacts": [{
                "stable-id": "lakecat:view:local:default:active_customers",
                "osi-hash": view_hash
            }],
            "view-version-receipts": [{
                "stable-id": "lakecat:view:local:default:active_customers",
                "view-version": 1,
                "receipt-hash": receipt_hash,
                "receipt-chain-hash": receipt_chain_hash
            }],
            "standards": [
                "Iceberg REST",
                "Croissant",
                "CDIF",
                "OSI handoff",
                "ODRL",
                "Grust catalog graph",
                "OpenLineage"
            ]
        }
    })
}

pub(crate) fn valid_lineage_summary_querygraph_bootstrap_event(event_id: &str) -> OutboxEvent {
    let principal = Principal::new("agent:reader", PrincipalKind::Agent).unwrap();
    let mut payload = valid_querygraph_bootstrap_payload(principal);
    payload["audit-event-id"] = json!(format!("audit-{event_id}"));
    OutboxEvent {
        event_id: event_id.to_string(),
        sink: "lakecat.lineage-and-graph".to_string(),
        event_type: "querygraph.bootstrap".to_string(),
        payload,
        created_at: chrono::Utc::now(),
        delivered_at: None,
    }
}

pub(crate) fn merge_json_object(target: &mut serde_json::Value, patch: serde_json::Value) {
    let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) else {
        *target = patch;
        return;
    };
    for (key, value) in patch {
        if value.is_object() {
            merge_json_object(
                target.entry(key).or_insert_with(|| json!({})),
                value.clone(),
            );
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

pub(crate) fn assert_config_defaults_include(defaults: &serde_json::Value, key: &str, value: &str) {
    let defaults = defaults
        .as_array()
        .expect("config defaults should be an array");
    assert!(
        defaults.iter().any(|entry| {
            entry.get("key").and_then(serde_json::Value::as_str) == Some(key)
                && entry.get("value").and_then(serde_json::Value::as_str) == Some(value)
        }),
        "config defaults should include {key}={value}: {defaults:?}"
    );
}

pub(crate) fn assert_config_endpoints_include(endpoints: &serde_json::Value, expected: &str) {
    let endpoints = endpoints
        .as_array()
        .expect("config endpoints should be an array");
    assert!(
        endpoints
            .iter()
            .any(|endpoint| endpoint.as_str() == Some(expected)),
        "config endpoints should include {expected}: {endpoints:?}"
    );
}

#[cfg(feature = "sail-local")]
pub(crate) struct LocalManifestFixture {
    pub(crate) root: std::path::PathBuf,
    pub(crate) table_location: String,
    pub(crate) metadata_location: String,
    pub(crate) delete_file_path: String,
    pub(crate) metadata: serde_json::Value,
}

#[cfg(feature = "sail-local")]
pub(crate) fn local_manifest_fixture() -> LocalManifestFixture {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use sail_iceberg::spec::{
        DataContentType, DataFile, DataFileFormat, FormatVersion, ManifestContentType,
        ManifestFile, ManifestListWriter, ManifestMetadata, ManifestWriterBuilder, TableMetadata,
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
    let table_metadata = TableMetadata::from_json(&serde_json::to_vec(&metadata).unwrap()).unwrap();
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
