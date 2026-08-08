use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
use lakecat_lineage::{
    HashOnlyLineageSink, LineageEvent, LineageEventType, LineageSink, open_lineage_event,
};
use serde_json::json;

fn table_ident() -> TableIdent {
    TableIdent::new(
        WarehouseName::new("local").expect("static warehouse"),
        Namespace::new(vec!["default".to_string()]).expect("static namespace"),
        TableName::new("events").expect("static table"),
    )
}

fn event(field_count: usize) -> LineageEvent {
    let fields = (0..field_count)
        .map(|index| {
            json!({
                "id": index + 1,
                "name": format!("field_{index}"),
                "type": "string",
            })
        })
        .collect::<Vec<_>>();
    LineageEvent::new(
        LineageEventType::TableCommitted,
        Principal::anonymous(),
        Some(table_ident()),
        json!({
            "metadata-location": "s3://warehouse/default/events/metadata/00001.json",
            "fields": fields,
        }),
    )
}

fn bench_lineage_events(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let sink = HashOnlyLineageSink::new();
    let mut group = c.benchmark_group("lineage_events");
    for field_count in [1, 100, 1_000] {
        let event = event(field_count);
        group.throughput(Throughput::Elements(field_count as u64));
        group.bench_with_input(
            BenchmarkId::new("open_lineage", field_count),
            &field_count,
            |b, _| b.iter(|| open_lineage_event(black_box(&event))),
        );
        group.bench_with_input(
            BenchmarkId::new("hash_sink", field_count),
            &field_count,
            |b, _| {
                b.to_async(&runtime).iter_batched(
                    || event.clone(),
                    |event| sink.emit(black_box(event)),
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_lineage_events);
criterion_main!(benches);
