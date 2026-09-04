use std::sync::Arc;

use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use grust_graph::MemoryGraphStore;
use lakecat_core::{Namespace, TableIdent, TableName, WarehouseName};
use lakecat_graph::grust_integration::{GrustCatalogGraphSink, graph_event_to_grust};
use lakecat_graph::{
    CatalogGraphSink, GraphAction, GraphEvent, column_stable_id, commit_stable_id,
    snapshot_stable_id,
};
use serde_json::json;

fn table_ident() -> TableIdent {
    TableIdent::new(
        WarehouseName::new("local").expect("static warehouse"),
        Namespace::new(vec!["default".to_string()]).expect("static namespace"),
        TableName::new("events").expect("static table"),
    )
}

fn event(property_count: usize) -> GraphEvent {
    let properties = (0..property_count)
        .map(|index| (format!("property_{index}"), json!(format!("value_{index}"))))
        .collect::<serde_json::Map<_, _>>();
    GraphEvent::table(
        GraphAction::Committed,
        table_ident(),
        serde_json::Value::Object(properties),
    )
    .with_event_id("lakecat:outbox:benchmark")
}

fn bench_graph_events(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("benchmark runtime");
    let sink = GrustCatalogGraphSink::new(Arc::new(MemoryGraphStore::new()));
    let mut group = c.benchmark_group("graph_events");
    for property_count in [1, 100, 1_000] {
        let event = event(property_count);
        group.throughput(Throughput::Elements(property_count as u64));
        group.bench_with_input(
            BenchmarkId::new("to_grust", property_count),
            &property_count,
            |b, _| b.iter(|| graph_event_to_grust(black_box(&event))),
        );
        group.bench_with_input(
            BenchmarkId::new("memory_sink", property_count),
            &property_count,
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

fn bench_table_stable_ids(c: &mut Criterion) {
    let table = table_ident();
    c.bench_function("graph_table_stable_ids", |b| {
        b.iter(|| {
            (
                commit_stable_id(black_box(&table), black_box(42)),
                column_stable_id(black_box(&table), black_box("17")),
                snapshot_stable_id(black_box(&table), black_box("9001")),
            )
        });
    });
}

criterion_group!(benches, bench_graph_events, bench_table_stable_ids);
criterion_main!(benches);
