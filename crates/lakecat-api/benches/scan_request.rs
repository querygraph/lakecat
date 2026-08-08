use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use lakecat_api::{NormalizedPlanTableScanRequest, PlanTableScanRequest};
use serde_json::json;

fn scan_request(field_count: usize) -> PlanTableScanRequest {
    let fields = (0..field_count)
        .map(|index| format!("field_{index}"))
        .collect::<Vec<_>>();
    PlanTableScanRequest {
        projection: Vec::new(),
        select: fields.clone(),
        filters: (0..field_count)
            .map(|index| {
                json!({
                    "type": "eq",
                    "term": format!("field_{index}"),
                    "value": index,
                })
            })
            .collect(),
        filter: Some(json!({"type": "always-true"})),
        limit: Some(10_000),
        snapshot_id: Some(42),
        case_sensitive: Some(true),
        use_snapshot_schema: Some(true),
        start_snapshot_id: None,
        end_snapshot_id: None,
        stats_fields: fields,
    }
}

fn normalize_request(request: PlanTableScanRequest) -> NormalizedPlanTableScanRequest {
    request
        .into_normalized()
        .expect("benchmark scan mode is valid")
}

fn bench_scan_request(c: &mut Criterion) {
    let mut normalization = c.benchmark_group("api_scan_request_normalization");
    for field_count in [1, 100, 256] {
        let request = scan_request(field_count);
        normalization.throughput(Throughput::Elements(field_count as u64));
        normalization.bench_with_input(
            BenchmarkId::from_parameter(field_count),
            &field_count,
            |b, _| {
                b.iter_batched(
                    || request.clone(),
                    |request| black_box(normalize_request(request)),
                    BatchSize::SmallInput,
                );
            },
        );
    }
    normalization.finish();

    let mut decoding = c.benchmark_group("api_scan_request_decode");
    for field_count in [1, 100, 256] {
        let encoded =
            serde_json::to_vec(&scan_request(field_count)).expect("encode benchmark scan request");
        decoding.throughput(Throughput::Bytes(encoded.len() as u64));
        decoding.bench_with_input(
            BenchmarkId::from_parameter(field_count),
            &field_count,
            |b, _| {
                b.iter(|| {
                    serde_json::from_slice::<PlanTableScanRequest>(black_box(&encoded))
                        .expect("decode benchmark scan request")
                });
            },
        );
    }
    decoding.finish();
}

criterion_group!(benches, bench_scan_request);
criterion_main!(benches);
