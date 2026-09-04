use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, Bytes};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use http::{Request, StatusCode};
use lakecat_core::sail::{
    CommitPlan, CommitPreparationRequest, FetchScanTasksPlan, FetchScanTasksRequest,
    SailCatalogEngine, ScanPlan, ScanPlanningRequest,
};
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, PrincipalKind, TableIdent, TableName,
    WarehouseName,
};
use lakecat_graph::NoopCatalogGraphSink;
use lakecat_lineage::HashOnlyLineageSink;
use lakecat_security::AllowAllGovernanceEngine;
use lakecat_service::{LakeCatState, TypeDidVerification, TypeDidVerifier, app};
use lakecat_store::{
    CatalogStore, GovernedScanGrant, PolicyBinding, TableCommit, TableCommitRecord, TableRecord,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const SNAPSHOT_ID: i64 = 42;
const TABLE_UUID: &str = "11111111-1111-1111-1111-111111111111";

struct ScanCase {
    router: Router,
    body: Bytes,
    governed: bool,
}

impl ScanCase {
    fn new(field_count: usize, governed: bool) -> Self {
        let warehouse = WarehouseName::new("local").expect("static warehouse");
        let namespace = Namespace::new(vec!["default".to_string()]).expect("static namespace");
        let table_name = TableName::new("events").expect("static table");
        let ident = TableIdent::new(warehouse.clone(), namespace.clone(), table_name.clone());
        let fields = field_names(field_count);
        let policy_binding = governed.then(|| {
            PolicyBinding::new(
                "benchmark-read",
                warehouse.clone(),
                Some(namespace),
                Some(table_name),
                true,
                json!({
                    "uid": "policy:benchmark-read",
                    "lakecat:read-restriction": {
                        "allowed-columns": fields,
                        "row-predicate": {"type": "always-true"},
                        "purpose": "benchmark",
                    },
                }),
            )
            .expect("benchmark policy binding")
        });
        let table = TableRecord::new(
            ident,
            "file:///benchmark/events".to_string(),
            Some("file:///benchmark/events/metadata/00000.json".to_string()),
            table_metadata(field_count),
            Principal::anonymous(),
        );
        let store: Arc<dyn CatalogStore> = Arc::new(BenchmarkCatalogStore {
            table,
            policy_binding,
        });
        let state = LakeCatState::new(warehouse, store)
            .with_integrations(
                Arc::new(BenchmarkSailEngine),
                AllowAllGovernanceEngine::new(),
                NoopCatalogGraphSink::new(),
                HashOnlyLineageSink::new(),
            )
            .with_typedid_verifier(Arc::new(BenchmarkTypeDidVerifier));
        let fields = field_names(field_count);
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "select": fields,
                "filters": (0..field_count)
                    .map(|index| json!({
                        "type": "eq",
                        "term": format!("field_{index}"),
                        "value": index,
                    }))
                    .collect::<Vec<_>>(),
                "filter": {"type": "always-true"},
                "limit": 10_000,
                "snapshot-id": SNAPSHOT_ID,
                "case-sensitive": true,
                "use-snapshot-schema": true,
                "stats-fields": fields,
            }))
            .expect("encode scan benchmark request"),
        );
        Self {
            router: app(state),
            body,
            governed,
        }
    }

    async fn plan(&self) {
        let mut request = Request::post("/catalog/v1/namespaces/default/tables/events/plan")
            .header("content-type", "application/json");
        if self.governed {
            request = request.header(
                "x-lakecat-typedid-envelope",
                r#"{"subject":"did:example:benchmark"}"#,
            );
        } else {
            request = request
                .header("x-lakecat-principal", "benchmark@example.com")
                .header("x-lakecat-principal-kind", "human");
        }
        let response = self
            .router
            .clone()
            .oneshot(
                request
                    .body(Body::from(self.body.clone()))
                    .expect("build scan benchmark request"),
            )
            .await
            .expect("scan request reaches service");
        assert_eq!(response.status(), StatusCode::OK);
        black_box(response);
    }
}

struct BenchmarkCatalogStore {
    table: TableRecord,
    policy_binding: Option<PolicyBinding>,
}

#[async_trait]
impl CatalogStore for BenchmarkCatalogStore {
    async fn create_namespace(
        &self,
        _warehouse: &WarehouseName,
        _namespace: Namespace,
    ) -> LakeCatResult<()> {
        Ok(())
    }

    async fn list_namespaces(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<Namespace>> {
        Ok((self.table.ident.warehouse == *warehouse)
            .then(|| self.table.ident.namespace.clone())
            .into_iter()
            .collect())
    }

    async fn list_tables(&self, warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>> {
        Ok((self.table.ident.warehouse == *warehouse)
            .then(|| self.table.clone())
            .into_iter()
            .collect())
    }

    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord> {
        Ok(table)
    }

    async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord> {
        if ident == &self.table.ident {
            Ok(self.table.clone())
        } else {
            Err(LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })
        }
    }

    async fn commit_table(
        &self,
        _ident: &TableIdent,
        _commit: TableCommit,
    ) -> LakeCatResult<TableRecord> {
        Err(LakeCatError::NotSupported(
            "benchmark store table commits".to_string(),
        ))
    }

    async fn table_commit_records(
        &self,
        _ident: &TableIdent,
        _start_version: u64,
        _end_version: Option<u64>,
    ) -> LakeCatResult<Vec<TableCommitRecord>> {
        Ok(Vec::new())
    }

    async fn save_governed_scan_grant(
        &self,
        grant: GovernedScanGrant,
    ) -> LakeCatResult<GovernedScanGrant> {
        grant.validate()?;
        Ok(grant)
    }

    async fn soft_delete_table(
        &self,
        _ident: &TableIdent,
        _principal: Principal,
        _authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord> {
        Err(LakeCatError::NotSupported(
            "benchmark store table deletion".to_string(),
        ))
    }

    async fn restore_table(
        &self,
        _ident: &TableIdent,
        _principal: Principal,
        _authorization_receipt: Option<Value>,
    ) -> LakeCatResult<TableRecord> {
        Err(LakeCatError::NotSupported(
            "benchmark store table restoration".to_string(),
        ))
    }

    async fn policy_bindings_for_table(
        &self,
        table: &TableIdent,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        Ok(self
            .policy_binding
            .iter()
            .filter(|binding| binding.applies_to_table(table))
            .cloned()
            .collect())
    }
}

struct BenchmarkSailEngine;

#[async_trait]
impl SailCatalogEngine for BenchmarkSailEngine {
    async fn prepare_commit(
        &self,
        _request: CommitPreparationRequest,
    ) -> LakeCatResult<CommitPlan> {
        Err(LakeCatError::NotSupported(
            "benchmark Sail commit preparation".to_string(),
        ))
    }

    async fn plan_scan(&self, request: ScanPlanningRequest) -> LakeCatResult<ScanPlan> {
        let projection_count = request.projection.len();
        let metadata_field_count = request
            .table_metadata
            .get("last-column-id")
            .and_then(Value::as_u64);
        Ok(ScanPlan {
            planned_by: "benchmark-sail".to_string(),
            snapshot_id: Some(SNAPSHOT_ID),
            scan_tasks: vec![json!({
                "task-type": "metadata",
                "plan-task": "lakecat:plan:benchmark",
                "projection-count": projection_count,
                "metadata-field-count": metadata_field_count,
            })],
            residual_filter: Some(json!({
                "projection": request.projection,
                "filters": request.filters,
            })),
        })
    }

    async fn fetch_scan_tasks(
        &self,
        _request: FetchScanTasksRequest,
    ) -> LakeCatResult<FetchScanTasksPlan> {
        Err(LakeCatError::NotSupported(
            "benchmark Sail task fetching".to_string(),
        ))
    }
}

struct BenchmarkTypeDidVerifier;

#[async_trait]
impl TypeDidVerifier for BenchmarkTypeDidVerifier {
    async fn verify(&self, _envelope_json: &str) -> Result<TypeDidVerification, LakeCatError> {
        Ok(TypeDidVerification {
            principal: Principal::new("did:example:benchmark", PrincipalKind::Agent)?,
            attestation: json!({"verified-by": "benchmark"}),
        })
    }
}

fn field_names(field_count: usize) -> Vec<String> {
    (0..field_count)
        .map(|index| format!("field_{index}"))
        .collect()
}

fn table_metadata(field_count: usize) -> Value {
    let fields = (0..field_count)
        .map(|index| {
            json!({
                "id": index + 1,
                "name": format!("field_{index}"),
                "type": if index == 0 { "long" } else { "string" },
                "required": index == 0,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "format-version": 2,
        "table-uuid": TABLE_UUID,
        "location": "file:///benchmark/events",
        "last-sequence-number": 7,
        "last-updated-ms": 1_710_000_000_000_i64,
        "last-column-id": field_count,
        "schemas": [{
            "type": "struct",
            "schema-id": 0,
            "fields": fields,
        }],
        "current-schema-id": 0,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
        "current-snapshot-id": SNAPSHOT_ID,
        "snapshots": [{
            "snapshot-id": SNAPSHOT_ID,
            "sequence-number": 7,
            "timestamp-ms": 1_710_000_000_000_i64,
            "manifest-list": "file:///benchmark/events/metadata/snap-42.avro",
            "summary": {"operation": "append"},
            "schema-id": 0,
        }],
        "snapshot-log": [{
            "timestamp-ms": 1_710_000_000_000_i64,
            "snapshot-id": SNAPSHOT_ID,
        }],
        "metadata-log": [],
        "sort-orders": [{"order-id": 0, "fields": []}],
        "default-sort-order-id": 0,
        "refs": {},
    })
}

fn bench_scan_path(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let cases = [
        ("unrestricted", 1, ScanCase::new(1, false)),
        ("unrestricted", 100, ScanCase::new(100, false)),
        ("unrestricted", 256, ScanCase::new(256, false)),
        ("governed", 1, ScanCase::new(1, true)),
        ("governed", 100, ScanCase::new(100, true)),
        ("governed", 256, ScanCase::new(256, true)),
    ];
    let mut group = c.benchmark_group("service_scan_path");
    group.sample_size(20);
    for (mode, field_count, case) in &cases {
        group.throughput(Throughput::Elements(*field_count as u64));
        group.bench_with_input(
            BenchmarkId::new(*mode, format!("{field_count}_fields")),
            case,
            |b, case| b.to_async(&runtime).iter(|| case.plan()),
        );
    }
    group.finish();
}

criterion_group!(benches, bench_scan_path);
criterion_main!(benches);
