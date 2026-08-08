use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use lakecat_core::governed_scan::{
    GovernedScanCatalogIdentity, GovernedScanProof, GovernedScanProofEvidence,
    governed_evidence_digest, governed_scan_digests,
};
use lakecat_core::{Namespace, TableIdent, TableName, WarehouseName, content_hash_json};
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

fn proof_evidence(projection_count: usize) -> GovernedScanProofEvidence {
    let digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    GovernedScanProofEvidence {
        catalog_identity: GovernedScanCatalogIdentity::new("lakecat://local")
            .expect("benchmark catalog identity"),
        table: TableIdent::new(
            WarehouseName::new("local").expect("static warehouse"),
            Namespace::new(vec!["default".to_string()]).expect("static namespace"),
            TableName::new("events").expect("static table"),
        ),
        table_version: 7,
        snapshot_id: 42,
        plan_task_digest: digest.to_string(),
        principal_subject: "agent:benchmark".to_string(),
        purpose: "performance-analysis".to_string(),
        effective_projection: (0..projection_count)
            .map(|index| format!("field_{index}"))
            .collect(),
        identity_context_digest: digest.to_string(),
        authorization_receipt_digest: digest.to_string(),
        policy_decision_digest: digest.to_string(),
    }
}

fn proof(projection_count: usize) -> GovernedScanProof {
    GovernedScanProof::issue(proof_evidence(projection_count)).expect("issue benchmark proof")
}

fn bench_governed_scan_evidence(c: &mut Criterion) {
    let mut group = c.benchmark_group("governed_scan_evidence");
    for projection_count in [1, 100, 256] {
        let evidence = proof_evidence(projection_count);
        let proof = proof(projection_count);
        let digest_value = json!({
            "projection": evidence.effective_projection,
            "metadata": metadata(projection_count),
        });
        group.throughput(Throughput::Elements(projection_count as u64));
        group.bench_with_input(
            BenchmarkId::new("domain_digest", projection_count),
            &projection_count,
            |b, _| {
                b.iter(|| {
                    governed_evidence_digest(
                        black_box("lakecat.benchmark.digest.v1"),
                        black_box(&digest_value),
                    )
                    .expect("digest benchmark evidence")
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("issue", projection_count),
            &projection_count,
            |b, _| {
                b.iter_batched(
                    || evidence.clone(),
                    |evidence| {
                        GovernedScanProof::issue(black_box(evidence))
                            .expect("issue benchmark proof")
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        group.bench_with_input(
            BenchmarkId::new("validate_integrity", projection_count),
            &projection_count,
            |b, _| {
                b.iter(|| {
                    black_box(&proof)
                        .validate_integrity()
                        .expect("validate benchmark proof")
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("snapshot_and_scope", projection_count),
            &projection_count,
            |b, _| {
                b.iter(|| {
                    governed_scan_digests(black_box(&proof))
                        .expect("derive benchmark governed scan digests")
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_json_evidence, bench_governed_scan_evidence);
criterion_main!(benches);
