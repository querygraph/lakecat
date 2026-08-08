use std::collections::BTreeMap;
use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
use lakecat_store::turso_store::TursoCatalogStore;
use lakecat_store::{
    CatalogStore, PolicyBinding, ProjectRecord, ServerRecord, ViewRecord, ViewVersionReceipt,
};
use serde_json::json;

struct QueryGraphReadCase {
    store: Arc<TursoCatalogStore>,
    warehouse: WarehouseName,
    namespace: Namespace,
    tables: Vec<TableIdent>,
    views: Vec<TableName>,
}

impl QueryGraphReadCase {
    async fn new(item_count: usize) -> Self {
        let store = TursoCatalogStore::in_memory()
            .await
            .expect("create Turso benchmark store");
        let warehouse = WarehouseName::new("local").expect("static warehouse");
        let namespace = Namespace::new(vec!["default".to_string()]).expect("static namespace");
        store
            .create_namespace(&warehouse, namespace.clone())
            .await
            .expect("create benchmark namespace");
        let mut tables = Vec::with_capacity(item_count);
        let mut views = Vec::with_capacity(item_count);

        for index in 0..item_count {
            let table = TableName::new(format!("table_{index:04}")).expect("benchmark table name");
            let ident = TableIdent::new(warehouse.clone(), namespace.clone(), table.clone());
            store
                .upsert_policy_binding(
                    PolicyBinding::new(
                        format!("policy-{index:04}"),
                        warehouse.clone(),
                        Some(namespace.clone()),
                        Some(table),
                        true,
                        json!({
                            "uid": format!("policy:benchmark:{index:04}"),
                            "permission": [{"action": "read"}],
                        }),
                    )
                    .expect("benchmark policy binding"),
                )
                .await
                .expect("persist benchmark policy binding");
            tables.push(ident);

            let view_name =
                TableName::new(format!("view_{index:04}")).expect("benchmark view name");
            store
                .upsert_view(
                    ViewRecord::new(
                        warehouse.clone(),
                        namespace.clone(),
                        view_name.clone(),
                        format!("select * from table_{index:04}"),
                        "spark",
                        Some(1),
                        BTreeMap::new(),
                        Principal::anonymous(),
                    )
                    .expect("benchmark view"),
                )
                .await
                .expect("persist benchmark view");
            views.push(view_name);
        }

        Self {
            store,
            warehouse,
            namespace,
            tables,
            views,
        }
    }

    async fn individual_policy_bindings(&self) -> Vec<Vec<PolicyBinding>> {
        let mut bindings = Vec::with_capacity(self.tables.len());
        for table in &self.tables {
            bindings.push(
                self.store
                    .policy_bindings_for_table(table)
                    .await
                    .expect("read benchmark table policies"),
            );
        }
        bindings
    }

    async fn bulk_policy_bindings(&self) -> Vec<Vec<PolicyBinding>> {
        self.store
            .policy_bindings_for_tables(&self.tables)
            .await
            .expect("read benchmark table policies in bulk")
    }

    async fn individual_view_receipts(&self) -> Vec<ViewVersionReceipt> {
        let mut receipts = Vec::with_capacity(self.views.len());
        for view in &self.views {
            receipts.extend(
                self.store
                    .list_view_version_receipts(&self.warehouse, &self.namespace, view)
                    .await
                    .expect("read benchmark view receipts"),
            );
        }
        receipts
    }

    async fn namespace_view_receipts(&self) -> Vec<ViewVersionReceipt> {
        self.store
            .list_namespace_view_version_receipts(&self.warehouse, &self.namespace)
            .await
            .expect("read benchmark namespace view receipts")
    }
}

struct TenantReadCase {
    store: Arc<TursoCatalogStore>,
    project_id: String,
    server_id: String,
}

impl TenantReadCase {
    async fn new(item_count: usize) -> Self {
        let store = TursoCatalogStore::in_memory()
            .await
            .expect("create Turso benchmark store");
        for index in 0..item_count {
            let server_id = format!("server-{index:04}");
            store
                .upsert_server(
                    ServerRecord::new(
                        server_id.clone(),
                        Some(format!("Server {index:04}")),
                        None,
                        BTreeMap::new(),
                        Principal::anonymous(),
                    )
                    .expect("benchmark server"),
                )
                .await
                .expect("persist benchmark server");
            store
                .upsert_project(
                    ProjectRecord::new(
                        format!("project-{index:04}"),
                        Some(server_id),
                        Some(format!("Project {index:04}")),
                        BTreeMap::new(),
                        Principal::anonymous(),
                    )
                    .expect("benchmark project"),
                )
                .await
                .expect("persist benchmark project");
        }
        let target_index = item_count - 1;
        Self {
            store,
            project_id: format!("project-{target_index:04}"),
            server_id: format!("server-{target_index:04}"),
        }
    }

    async fn list_and_find(&self) -> (ProjectRecord, ServerRecord) {
        let project = self
            .store
            .list_projects()
            .await
            .expect("list benchmark projects")
            .into_iter()
            .find(|project| project.project_id == self.project_id)
            .expect("target benchmark project");
        let server = self
            .store
            .list_servers()
            .await
            .expect("list benchmark servers")
            .into_iter()
            .find(|server| server.server_id == self.server_id)
            .expect("target benchmark server");
        (project, server)
    }

    async fn load_points(&self) -> (ProjectRecord, ServerRecord) {
        let project = self
            .store
            .load_project(&self.project_id)
            .await
            .expect("load benchmark project");
        let server = self
            .store
            .load_server(&self.server_id)
            .await
            .expect("load benchmark server");
        (project, server)
    }
}

struct WarehouseViewReadCase {
    store: Arc<TursoCatalogStore>,
    warehouse: WarehouseName,
    namespaces: Vec<Namespace>,
}

impl WarehouseViewReadCase {
    async fn new(item_count: usize) -> Self {
        let store = TursoCatalogStore::in_memory()
            .await
            .expect("create Turso benchmark store");
        let warehouse = WarehouseName::new("local").expect("static warehouse");
        let namespace_count = item_count.clamp(1, 16);
        let namespaces = (0..namespace_count)
            .map(|index| {
                Namespace::new(vec![format!("namespace_{index:02}")]).expect("benchmark namespace")
            })
            .collect::<Vec<_>>();
        for namespace in &namespaces {
            store
                .create_namespace(&warehouse, namespace.clone())
                .await
                .expect("persist benchmark namespace");
        }
        for index in 0..item_count {
            let namespace = namespaces[index % namespace_count].clone();
            store
                .upsert_view(
                    ViewRecord::new(
                        warehouse.clone(),
                        namespace,
                        TableName::new(format!("view_{index:04}")).expect("benchmark view name"),
                        format!("select * from table_{index:04}"),
                        "spark",
                        Some(1),
                        BTreeMap::new(),
                        Principal::anonymous(),
                    )
                    .expect("benchmark view"),
                )
                .await
                .expect("persist benchmark view");
        }
        Self {
            store,
            warehouse,
            namespaces,
        }
    }

    async fn namespace_views(&self) -> Vec<ViewRecord> {
        let mut views = Vec::new();
        for namespace in &self.namespaces {
            views.extend(
                self.store
                    .list_views(&self.warehouse, namespace)
                    .await
                    .expect("list benchmark namespace views"),
            );
        }
        views
    }

    async fn warehouse_views(&self) -> Vec<ViewRecord> {
        self.store
            .list_warehouse_views(&self.warehouse)
            .await
            .expect("list benchmark warehouse views")
    }

    async fn namespace_receipts(&self) -> Vec<ViewVersionReceipt> {
        let mut receipts = Vec::new();
        for namespace in &self.namespaces {
            receipts.extend(
                self.store
                    .list_namespace_view_version_receipts(&self.warehouse, namespace)
                    .await
                    .expect("list benchmark namespace receipts"),
            );
        }
        receipts
    }

    async fn warehouse_receipts(&self) -> Vec<ViewVersionReceipt> {
        self.store
            .list_warehouse_view_version_receipts(&self.warehouse)
            .await
            .expect("list benchmark warehouse receipts")
    }
}

fn bench_querygraph_reads(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let cases = [
        (1, runtime.block_on(QueryGraphReadCase::new(1))),
        (64, runtime.block_on(QueryGraphReadCase::new(64))),
        (256, runtime.block_on(QueryGraphReadCase::new(256))),
    ];
    let mut group = c.benchmark_group("turso_querygraph_reads");
    group.sample_size(20);
    for (item_count, case) in &cases {
        group.throughput(Throughput::Elements(*item_count as u64));
        group.bench_with_input(
            BenchmarkId::new("policies_individual", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.individual_policy_bindings().await) });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("policies_bulk", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.bulk_policy_bindings().await) });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("view_receipts_individual", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.individual_view_receipts().await) });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("view_receipts_namespace", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.namespace_view_receipts().await) });
            },
        );
    }
    group.finish();
}

fn bench_tenant_reads(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let cases = [
        (1, runtime.block_on(TenantReadCase::new(1))),
        (64, runtime.block_on(TenantReadCase::new(64))),
        (256, runtime.block_on(TenantReadCase::new(256))),
    ];
    let mut group = c.benchmark_group("turso_tenant_reads");
    group.sample_size(20);
    for (item_count, case) in &cases {
        group.bench_with_input(
            BenchmarkId::new("list_and_find", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.list_and_find().await) });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("load_points", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.load_points().await) });
            },
        );
    }
    group.finish();
}

fn bench_warehouse_view_reads(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let cases = [
        (1, runtime.block_on(WarehouseViewReadCase::new(1))),
        (64, runtime.block_on(WarehouseViewReadCase::new(64))),
        (256, runtime.block_on(WarehouseViewReadCase::new(256))),
    ];
    let mut group = c.benchmark_group("turso_warehouse_view_reads");
    group.sample_size(20);
    for (item_count, case) in &cases {
        group.throughput(Throughput::Elements(*item_count as u64));
        group.bench_with_input(
            BenchmarkId::new("namespace_views", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.namespace_views().await) });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("warehouse_views", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.warehouse_views().await) });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("namespace_receipts", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.namespace_receipts().await) });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("warehouse_receipts", item_count),
            case,
            |b, case| {
                b.to_async(&runtime)
                    .iter(|| async { black_box(case.warehouse_receipts().await) });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_querygraph_reads,
    bench_tenant_reads,
    bench_warehouse_view_reads
);
criterion_main!(benches);
