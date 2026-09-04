use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use http::{Request, StatusCode};
use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
use lakecat_service::{LakeCatState, app};
use lakecat_store::turso_store::TursoCatalogStore;
use lakecat_store::{CatalogStore, MemoryCatalogStore, TableRecord};
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

const TABLE_UUID: &str = "11111111-1111-1111-1111-111111111111";

struct CommitCase {
    router: Router,
    body: String,
    _metadata_root: TempDir,
}

impl CommitCase {
    async fn new(store: Arc<dyn CatalogStore>, field_count: usize) -> Self {
        let metadata_root = tempfile::tempdir().expect("create metadata benchmark directory");
        let table_path = metadata_root.path().join("events");
        let table_location = url::Url::from_directory_path(&table_path)
            .expect("temporary table path converts to a file URL")
            .to_string()
            .trim_end_matches('/')
            .to_string();
        let warehouse = WarehouseName::new("local").expect("valid warehouse");
        let namespace = "default".parse::<Namespace>().expect("valid namespace");
        let ident = TableIdent::new(
            warehouse.clone(),
            namespace.clone(),
            TableName::new("events").expect("valid table"),
        );
        store
            .create_namespace(&warehouse, namespace)
            .await
            .expect("create benchmark namespace");
        store
            .create_table(TableRecord::new(
                ident,
                table_location.clone(),
                Some(format!("{table_location}/metadata/00000.metadata.json")),
                table_metadata(&table_location, field_count),
                Principal::anonymous(),
            ))
            .await
            .expect("create benchmark table");
        let body = serde_json::to_string(&json!({
            "requirements": [{"type": "assert-table-uuid", "uuid": TABLE_UUID}],
            "updates": [{
                "action": "set-properties",
                "updates": {"bench.counter": "1"},
            }],
        }))
        .expect("encode commit request");
        Self {
            router: app(LakeCatState::new(warehouse, store)),
            body,
            _metadata_root: metadata_root,
        }
    }

    async fn commit(&self) {
        let request = Request::post("/catalog/v1/namespaces/default/tables/events/commit")
            .header("content-type", "application/json")
            .body(Body::from(self.body.clone()))
            .expect("build benchmark request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("commit request reaches the service");
        assert_eq!(response.status(), StatusCode::OK);
        black_box(response);
    }
}

fn table_metadata(table_location: &str, field_count: usize) -> Value {
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
        "location": table_location,
        "last-sequence-number": 0,
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
        "last-partition-id": 0,
        "properties": {"bench.counter": "0"},
        "current-snapshot-id": null,
        "snapshots": [],
        "snapshot-log": [],
        "metadata-log": [],
        "sort-orders": [{"order-id": 0, "fields": []}],
        "default-sort-order-id": 0,
        "refs": {},
    })
}

fn bench_commit_path(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let turso_one = runtime.block_on(async {
        let store = TursoCatalogStore::in_memory()
            .await
            .expect("create Turso store");
        CommitCase::new(store, 1).await
    });
    let turso_hundred = runtime.block_on(async {
        let store = TursoCatalogStore::in_memory()
            .await
            .expect("create Turso store");
        CommitCase::new(store, 100).await
    });
    let memory_one = runtime.block_on(CommitCase::new(MemoryCatalogStore::new(), 1));
    let memory_hundred = runtime.block_on(CommitCase::new(MemoryCatalogStore::new(), 100));

    let mut group = c.benchmark_group("service_commit_path");
    group.sample_size(20);
    group.throughput(Throughput::Elements(1));
    for (store, field_count, case) in [
        ("turso_sail_local_file", 1, &turso_one),
        ("turso_sail_local_file", 100, &turso_hundred),
        ("memory_sail_local_file", 1, &memory_one),
        ("memory_sail_local_file", 100, &memory_hundred),
    ] {
        group.bench_with_input(
            BenchmarkId::new(store, format!("{field_count}_fields")),
            case,
            |b, case| {
                b.to_async(&runtime).iter(|| case.commit());
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_commit_path);
criterion_main!(benches);
