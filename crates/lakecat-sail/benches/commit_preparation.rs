use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
use lakecat_sail::sail_integration::{SailRestModelCatalogEngine, inspect_sail_table_metadata};
use lakecat_sail::{CommitPreparationRequest, SailCatalogEngine};
use serde_json::{Value, json};

fn table_ident() -> TableIdent {
    TableIdent::new(
        WarehouseName::new("local").expect("valid warehouse"),
        "default".parse::<Namespace>().expect("valid namespace"),
        TableName::new("events").expect("valid table"),
    )
}

fn sample_metadata(field_count: usize) -> Value {
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
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": "s3://warehouse/lakecat/events",
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

fn commit_request(metadata: &Value) -> CommitPreparationRequest {
    CommitPreparationRequest {
        table: table_ident(),
        principal: Principal::anonymous(),
        current_metadata_location: Some(
            "s3://warehouse/lakecat/events/metadata/00000.metadata.json".to_string(),
        ),
        new_metadata_location: None,
        current_metadata: metadata.clone(),
        new_metadata: None,
        requirements: vec![json!({
            "type": "assert-table-uuid",
            "uuid": "11111111-1111-1111-1111-111111111111",
        })],
        updates: vec![json!({
            "action": "set-properties",
            "updates": {"bench.counter": "1"},
        })],
    }
}

fn bench_commit_preparation(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let engine = SailRestModelCatalogEngine;
    let mut group = c.benchmark_group("sail_commit_preparation");
    group.sample_size(30);
    group.throughput(Throughput::Elements(1));

    for fields in [1, 100] {
        let metadata = sample_metadata(fields);
        let request = commit_request(&metadata);
        group.bench_function(format!("set_properties_{fields}_fields"), |b| {
            b.to_async(&runtime)
                .iter(|| engine.prepare_commit(black_box(request.clone())));
        });
    }
    group.finish();

    let metadata = sample_metadata(100);
    c.bench_function("sail_metadata_inspection_100_fields", |b| {
        b.iter(|| {
            black_box(inspect_sail_table_metadata(black_box(&metadata)))
                .expect("valid benchmark metadata")
        });
    });
}

criterion_group!(benches, bench_commit_preparation);
criterion_main!(benches);
