use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lakecat_core::content_hash_json;
use serde_json::{Value, json};

fn metadata(field_count: usize) -> Value {
    let fields = (0..field_count)
        .map(|index| {
            json!({
                "id": index + 1,
                "name": format!("field_{index}"),
                "type": "string",
                "required": false,
                "doc": "Representative Iceberg field metadata",
            })
        })
        .collect::<Vec<_>>();
    json!({
        "format-version": 2,
        "table-uuid": "11111111-1111-1111-1111-111111111111",
        "location": "s3://warehouse/lakecat/events",
        "last-updated-ms": 1_710_000_000_000_i64,
        "last-column-id": field_count,
        "schemas": [{"type": "struct", "schema-id": 0, "fields": fields}],
        "current-schema-id": 0,
        "partition-specs": [{"spec-id": 0, "fields": []}],
        "properties": {"bench.counter": "1"},
        "snapshots": [],
        "metadata-log": [],
    })
}

fn bench_json_evidence(c: &mut Criterion) {
    let mut hash_group = c.benchmark_group("content_hash_json");
    hash_group.sample_size(50);
    for fields in [1, 100] {
        let value = metadata(fields);
        let bytes = serde_json::to_vec(&value).expect("benchmark metadata serializes");
        hash_group.throughput(Throughput::Bytes(bytes.len() as u64));
        hash_group.bench_with_input(BenchmarkId::from_parameter(fields), &value, |b, value| {
            b.iter(|| content_hash_json(black_box(value)).expect("hash metadata"));
        });
    }
    hash_group.finish();

    let value = metadata(100);
    let mut encoding_group = c.benchmark_group("metadata_json_encoding_100_fields");
    encoding_group.sample_size(50);
    encoding_group.bench_function("compact", |b| {
        b.iter(|| serde_json::to_vec(black_box(&value)).expect("encode compact metadata"));
    });
    encoding_group.bench_function("pretty", |b| {
        b.iter(|| serde_json::to_vec_pretty(black_box(&value)).expect("encode pretty metadata"));
    });
    encoding_group.finish();
}

criterion_group!(benches, bench_json_evidence);
criterion_main!(benches);
