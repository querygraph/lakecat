use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
use lakecat_store::turso_store::TursoCatalogStore;
use lakecat_store::{CatalogStore, TableCommit, TableRecord};
use serde_json::{Value, json};

fn table_ident() -> TableIdent {
    TableIdent::new(
        WarehouseName::new("local").expect("valid warehouse"),
        "default".parse::<Namespace>().expect("valid namespace"),
        TableName::new("events").expect("valid table"),
    )
}

fn table_metadata() -> Value {
    json!({
        "format-version": 2,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": "s3://warehouse/lakecat/events",
        "last-sequence-number": 0,
        "last-updated-ms": 1_710_000_000_000_i64,
        "last-column-id": 1,
        "schemas": [{
            "type": "struct",
            "schema-id": 0,
            "fields": [{"id": 1, "name": "id", "type": "long", "required": false}],
        }],
        "current-schema-id": 0,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "default-spec-id": 0,
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

fn commit(metadata: &Value) -> TableCommit {
    TableCommit {
        requirements: vec![json!({
            "type": "assert-table-uuid",
            "uuid": "11111111-1111-1111-1111-111111111111",
        })],
        updates: vec![json!({
            "action": "set-properties",
            "updates": {"bench.counter": "1"},
        })],
        expected_previous_metadata_location: None,
        new_metadata_location: None,
        new_metadata: Some(metadata.clone()),
        idempotency_key: None,
        idempotency_request_hash: None,
        principal: Principal::anonymous(),
        authorization_receipt: Some(json!({
            "engine": "benchmark",
            "allowed": true,
            "policy_hash": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        })),
    }
}

fn bench_turso_commit(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let metadata = table_metadata();
    let ident = table_ident();
    let store = runtime.block_on(async {
        let store = TursoCatalogStore::in_memory()
            .await
            .expect("create Turso benchmark store");
        store
            .create_namespace(&ident.warehouse, ident.namespace.clone())
            .await
            .expect("create benchmark namespace");
        store
            .create_table(TableRecord::new(
                ident.clone(),
                "s3://warehouse/lakecat/events".to_string(),
                None,
                metadata.clone(),
                Principal::anonymous(),
            ))
            .await
            .expect("create benchmark table");
        store
    });
    let table_commit = commit(&metadata);

    let mut group = c.benchmark_group("turso_catalog_store");
    group.sample_size(20);
    group.throughput(Throughput::Elements(1));
    group.bench_function("commit_table", |b| {
        b.to_async(&runtime)
            .iter(|| store.commit_table(black_box(&ident), black_box(table_commit.clone())));
    });
    group.bench_function("load_table", |b| {
        b.to_async(&runtime)
            .iter(|| store.load_table(black_box(&ident)));
    });
    group.finish();
}

criterion_group!(benches, bench_turso_commit);
criterion_main!(benches);
