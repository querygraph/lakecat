use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use lakecat_core::{Namespace, Principal, TableIdent, TableName, WarehouseName};
use lakecat_querygraph::{
    QueryGraphViewReceiptEvidence, bootstrap_from_tables,
    bootstrap_from_tables_views_with_policy_bindings, catalog_graph_from_tables, graph_hash,
    table_only_querygraph_import_hash, table_projection_from_table, validate_view_receipt_evidence,
};
use lakecat_store::{TableRecord, ViewRecord};
use serde_json::json;

fn table(name: &str, field_count: usize) -> TableRecord {
    let fields = (0..field_count)
        .map(|index| {
            json!({
                "id": index + 1,
                "name": format!("field_{index}"),
                "type": "string",
                "required": index % 2 == 0,
                "doc": format!("Benchmark field {index}."),
            })
        })
        .collect::<Vec<_>>();
    TableRecord::new(
        TableIdent::new(
            WarehouseName::new("local").expect("static warehouse"),
            Namespace::new(vec!["default".to_string()]).expect("static namespace"),
            TableName::new(name).expect("benchmark table name"),
        ),
        format!("s3://warehouse/default/{name}"),
        Some(format!("s3://warehouse/default/{name}/metadata/00000.json")),
        json!({
            "format-version": 3,
            "current-schema-id": 1,
            "schemas": [{
                "schema-id": 1,
                "type": "struct",
                "fields": fields,
            }],
        }),
        Principal::anonymous(),
    )
}

fn tables(count: usize, field_count: usize) -> Vec<TableRecord> {
    (0..count)
        .map(|index| table(&format!("table_{index}"), field_count))
        .collect()
}

fn views(count: usize) -> Vec<ViewRecord> {
    (0..count)
        .map(|index| {
            ViewRecord::new(
                WarehouseName::new("local").expect("static warehouse"),
                Namespace::new(vec!["default".to_string()]).expect("static namespace"),
                TableName::new(format!("view_{index}")).expect("benchmark view name"),
                format!("select field_0 from table_{index}"),
                "sql",
                Some(1),
                BTreeMap::new(),
                Principal::anonymous(),
            )
            .expect("create benchmark view")
        })
        .collect()
}

fn bench_table_projection(c: &mut Criterion) {
    let mut group = c.benchmark_group("querygraph_table_projection");
    for field_count in [1, 100, 1_000] {
        let input = table("events", field_count);
        group.throughput(Throughput::Elements(field_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(field_count),
            &field_count,
            |b, _| {
                b.iter_batched(
                    || input.clone(),
                    |input| table_projection_from_table(black_box(input)),
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_catalog_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("querygraph_catalog");
    for table_count in [1, 64, 256] {
        let records = tables(table_count, 10);
        let projections = records
            .iter()
            .cloned()
            .map(table_projection_from_table)
            .collect::<Vec<_>>();
        let bundle = bootstrap_from_tables(
            WarehouseName::new("local").expect("static warehouse"),
            records.clone(),
        )
        .expect("build benchmark bundle");

        group.throughput(Throughput::Elements(table_count as u64));
        group.bench_with_input(
            BenchmarkId::new("graph", table_count),
            &table_count,
            |b, _| b.iter(|| catalog_graph_from_tables(black_box(&projections))),
        );
        group.bench_with_input(
            BenchmarkId::new("bootstrap", table_count),
            &table_count,
            |b, _| {
                b.iter_batched(
                    || records.clone(),
                    |records| {
                        bootstrap_from_tables(
                            WarehouseName::new("local").expect("static warehouse"),
                            black_box(records),
                        )
                        .expect("build benchmark bundle")
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        group.bench_with_input(
            BenchmarkId::new("graph_hash", table_count),
            &table_count,
            |b, _| {
                b.iter(|| {
                    graph_hash(black_box(&bundle.graph)).expect("hash benchmark catalog graph")
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("import_hash", table_count),
            &table_count,
            |b, _| {
                b.iter(|| {
                    table_only_querygraph_import_hash(
                        &bundle.warehouse,
                        &bundle.manifest,
                        &bundle.tables,
                        &bundle.graph,
                        &bundle.open_lineage,
                    )
                    .expect("hash benchmark import bundle")
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("construction_summary", table_count),
            &table_count,
            |b, _| {
                b.iter(|| {
                    black_box(&bundle)
                        .construction_summary()
                        .expect("summarize constructed benchmark bundle")
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("verify", table_count),
            &table_count,
            |b, _| {
                b.iter(|| {
                    black_box(&bundle)
                        .verify_manifest()
                        .expect("verify benchmark bundle")
                });
            },
        );
    }
    group.finish();
}

fn bench_view_receipts(c: &mut Criterion) {
    let mut group = c.benchmark_group("querygraph_view_receipts");
    for view_count in [1, 64, 256] {
        let bundle = bootstrap_from_tables_views_with_policy_bindings(
            WarehouseName::new("local").expect("static warehouse"),
            Vec::new(),
            views(view_count),
        )
        .expect("build benchmark view bundle");
        let evidence = bundle
            .views
            .iter()
            .map(|view| QueryGraphViewReceiptEvidence {
                stable_id: view.stable_id.clone(),
                view_version: view.view_version,
                receipt_hash: format!("receipt-{}", view.stable_id),
                receipt_chain_hash: format!("chain-{}", view.stable_id),
            })
            .collect::<Vec<_>>();
        let verified_bundle = bundle
            .clone()
            .with_view_receipt_evidence(evidence.clone())
            .expect("attach benchmark view evidence");

        group.throughput(Throughput::Elements(view_count as u64));
        group.bench_with_input(
            BenchmarkId::new("validate", view_count),
            &view_count,
            |b, _| {
                b.iter(|| {
                    validate_view_receipt_evidence(black_box(&bundle.views), black_box(&evidence))
                        .expect("validate benchmark view evidence")
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("attach", view_count),
            &view_count,
            |b, _| {
                b.iter_batched(
                    || (bundle.clone(), evidence.clone()),
                    |(bundle, evidence)| {
                        bundle
                            .with_view_receipt_evidence(black_box(evidence))
                            .expect("attach benchmark view evidence")
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        group.bench_with_input(
            BenchmarkId::new("verify", view_count),
            &view_count,
            |b, _| {
                b.iter(|| {
                    black_box(&verified_bundle)
                        .verify_manifest()
                        .expect("verify benchmark view bundle")
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_table_projection,
    bench_catalog_scale,
    bench_view_receipts
);
criterion_main!(benches);
use std::collections::BTreeMap;
