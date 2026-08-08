use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use chrono::Utc;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use http::{Request, StatusCode};
use lakecat_core::{
    LakeCatError, LakeCatResult, Namespace, Principal, TableIdent, TableName, WarehouseName,
    content_hash_json,
};
use lakecat_service::{LakeCatState, app};
use lakecat_store::{
    CatalogAuditEvent, CatalogStore, PolicyBinding, TableCommit, TableCommitRecord, TableRecord,
    ViewColumnRecord, ViewRecord, ViewVersionOperation, ViewVersionReceipt,
};
use serde_json::{Value, json};
use tower::ServiceExt;

struct BootstrapCase {
    router: Router,
}

impl BootstrapCase {
    fn new(item_count: usize) -> Self {
        let warehouse = WarehouseName::new("local").expect("static warehouse");
        let namespace_count = item_count.clamp(1, 16);
        let namespaces = (0..namespace_count)
            .map(|index| {
                Namespace::new(vec![format!("namespace_{index:02}")]).expect("benchmark namespace")
            })
            .collect::<Vec<_>>();
        let mut tables = Vec::with_capacity(item_count);
        let mut views = Vec::with_capacity(item_count);
        let mut policy_bindings = Vec::with_capacity(item_count);
        let mut view_receipts = Vec::with_capacity(item_count);

        for index in 0..item_count {
            let namespace = namespaces[index % namespace_count].clone();
            let table_name =
                TableName::new(format!("table_{index:04}")).expect("benchmark table name");
            let ident = TableIdent::new(warehouse.clone(), namespace.clone(), table_name.clone());
            tables.push(TableRecord::new(
                ident,
                format!("file:///benchmark/tables/{index:04}"),
                Some(format!(
                    "file:///benchmark/tables/{index:04}/metadata/00000.json"
                )),
                table_metadata(index),
                Principal::anonymous(),
            ));
            policy_bindings.push(
                PolicyBinding::new(
                    format!("policy-{index:04}"),
                    warehouse.clone(),
                    Some(namespace.clone()),
                    Some(table_name),
                    true,
                    json!({
                        "uid": format!("policy:benchmark:{index:04}"),
                        "permission": [{"action": "read"}],
                    }),
                )
                .expect("benchmark policy binding"),
            );

            let view_name =
                TableName::new(format!("view_{index:04}")).expect("benchmark view name");
            let view = ViewRecord::new(
                warehouse.clone(),
                namespace,
                view_name,
                format!("select field_0 from table_{index:04}"),
                "spark",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .and_then(|view| {
                view.with_columns(vec![ViewColumnRecord {
                    name: "field_0".to_string(),
                    data_type: json!("long"),
                    nullable: false,
                    comment: None,
                }])
            })
            .expect("benchmark view");
            view_receipts.push(view_receipt(&view));
            views.push(view);
        }

        let store: Arc<dyn CatalogStore> = Arc::new(BenchmarkCatalogStore {
            namespaces,
            tables,
            views,
            policy_bindings,
            view_receipts,
        });
        Self {
            router: app(LakeCatState::new(warehouse, store)),
        }
    }

    async fn bootstrap(&self) {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::get("/querygraph/v1/bootstrap")
                    .header("x-lakecat-principal", "benchmark@example.com")
                    .header("x-lakecat-principal-kind", "human")
                    .body(Body::empty())
                    .expect("build bootstrap benchmark request"),
            )
            .await
            .expect("bootstrap request reaches service");
        assert_eq!(response.status(), StatusCode::OK);
        black_box(response);
    }
}

struct BenchmarkCatalogStore {
    namespaces: Vec<Namespace>,
    tables: Vec<TableRecord>,
    views: Vec<ViewRecord>,
    policy_bindings: Vec<PolicyBinding>,
    view_receipts: Vec<ViewVersionReceipt>,
}

impl BenchmarkCatalogStore {
    fn table_policy_bindings(&self, table: &TableIdent) -> Vec<PolicyBinding> {
        self.policy_bindings
            .iter()
            .filter(|binding| binding.applies_to_table(table))
            .cloned()
            .collect()
    }
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

    async fn list_namespaces(&self, _warehouse: &WarehouseName) -> LakeCatResult<Vec<Namespace>> {
        Ok(self.namespaces.clone())
    }

    async fn list_tables(&self, _warehouse: &WarehouseName) -> LakeCatResult<Vec<TableRecord>> {
        Ok(self.tables.clone())
    }

    async fn create_table(&self, table: TableRecord) -> LakeCatResult<TableRecord> {
        Ok(table)
    }

    async fn load_table(&self, ident: &TableIdent) -> LakeCatResult<TableRecord> {
        self.tables
            .iter()
            .find(|table| table.ident == *ident)
            .cloned()
            .ok_or_else(|| LakeCatError::NotFound {
                object: "table",
                name: ident.stable_id(),
            })
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

    async fn list_views(
        &self,
        _warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewRecord>> {
        Ok(self
            .views
            .iter()
            .filter(|view| view.namespace == *namespace)
            .cloned()
            .collect())
    }

    async fn list_view_version_receipts(
        &self,
        _warehouse: &WarehouseName,
        namespace: &Namespace,
        view: &TableName,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        Ok(self
            .view_receipts
            .iter()
            .filter(|receipt| receipt.namespace == *namespace && receipt.name == *view)
            .cloned()
            .collect())
    }

    async fn list_namespace_view_version_receipts(
        &self,
        _warehouse: &WarehouseName,
        namespace: &Namespace,
    ) -> LakeCatResult<Vec<ViewVersionReceipt>> {
        Ok(self
            .view_receipts
            .iter()
            .filter(|receipt| receipt.namespace == *namespace)
            .cloned()
            .collect())
    }

    async fn list_policy_bindings(
        &self,
        _warehouse: &WarehouseName,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        Ok(self.policy_bindings.clone())
    }

    async fn policy_bindings_for_table(
        &self,
        table: &TableIdent,
    ) -> LakeCatResult<Vec<PolicyBinding>> {
        Ok(self.table_policy_bindings(table))
    }

    async fn policy_bindings_for_tables(
        &self,
        tables: &[TableIdent],
    ) -> LakeCatResult<Vec<Vec<PolicyBinding>>> {
        Ok(tables
            .iter()
            .map(|table| self.table_policy_bindings(table))
            .collect())
    }

    async fn record_audit_event(&self, event: CatalogAuditEvent) -> LakeCatResult<()> {
        event.validate_recordable()
    }
}

fn table_metadata(index: usize) -> Value {
    json!({
        "format-version": 2,
        "table-uuid": format!("00000000-0000-0000-0000-{index:012x}"),
        "location": format!("file:///benchmark/tables/{index:04}"),
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 0,
            "fields": [{
                "id": 1,
                "name": "field_0",
                "type": "long",
                "required": true,
            }],
        }],
        "current-schema-id": 0,
    })
}

fn view_receipt(view: &ViewRecord) -> ViewVersionReceipt {
    let view_hash = content_hash_json(
        &serde_json::to_value(view).expect("serialize benchmark view for receipt"),
    )
    .expect("hash benchmark view");
    let receipt = ViewVersionReceipt {
        stable_id: format!(
            "lakecat:view:{}:{}:{}",
            view.warehouse.as_str(),
            view.namespace.path(),
            view.name.as_str(),
        ),
        warehouse: view.warehouse.clone(),
        namespace: view.namespace.clone(),
        name: view.name.clone(),
        view_version: view.view_version,
        previous_view_version: None,
        previous_receipt_hash: None,
        operation: ViewVersionOperation::Upsert,
        view_hash,
        principal: Principal::anonymous(),
        recorded_at: Utc::now(),
    };
    receipt.validate().expect("benchmark view receipt");
    receipt
}

fn bench_querygraph_path(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let mut group = c.benchmark_group("service_querygraph_bootstrap");
    group.sample_size(20);
    for item_count in [1, 64, 256] {
        let case = BootstrapCase::new(item_count);
        group.throughput(Throughput::Elements(item_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{item_count}_tables_and_views")),
            &case,
            |b, case| b.to_async(&runtime).iter(|| case.bootstrap()),
        );
    }
    group.finish();
}

criterion_group!(benches, bench_querygraph_path);
criterion_main!(benches);
