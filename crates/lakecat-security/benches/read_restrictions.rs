use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use lakecat_security::ReadRestriction;
use serde_json::{Value, json};

fn columns(prefix: &str, count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("{prefix}_{index}"))
        .collect()
}

fn policy(uid: &str, allowed_columns: &[String]) -> Value {
    json!({
        "uid": uid,
        "lakecat:read-restriction": {
            "allowed-columns": allowed_columns,
            "max-credential-ttl-seconds": 300,
        }
    })
}

fn bench_read_restrictions(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_restrictions");
    for size in [16, 256, 1_024] {
        group.throughput(Throughput::Elements(size as u64));

        let allowed = columns("column", size);
        let requested = (0..size)
            .map(|index| {
                if index % 2 == 0 {
                    format!("column_{index}")
                } else {
                    format!("outside_{index}")
                }
            })
            .collect::<Vec<_>>();
        let restriction = ReadRestriction {
            allowed_columns: Some(allowed.clone()),
            ..ReadRestriction::unrestricted()
        };

        group.bench_with_input(
            BenchmarkId::new("effective_projection", size),
            &size,
            |b, _| {
                b.iter(|| {
                    restriction
                        .effective_projection(black_box(&requested))
                        .expect("benchmark projection must overlap")
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("effective_stats_fields", size),
            &size,
            |b, _| {
                b.iter(|| restriction.effective_stats_fields(black_box(&requested)));
            },
        );

        let direct_policy = policy("direct", &allowed);
        group.bench_with_input(BenchmarkId::new("parse_one_policy", size), &size, |b, _| {
            b.iter(|| {
                ReadRestriction::from_odrl_policies([black_box(&direct_policy)])
                    .expect("benchmark policy must be valid")
            });
        });

        let right = (size / 2..size + size / 2)
            .map(|index| format!("column_{index}"))
            .collect::<Vec<_>>();
        let left_policy = policy("left", &allowed);
        let right_policy = policy("right", &right);
        group.bench_with_input(
            BenchmarkId::new("compose_two_policies", size),
            &size,
            |b, _| {
                b.iter(|| {
                    ReadRestriction::from_odrl_policies([
                        black_box(&left_policy),
                        black_box(&right_policy),
                    ])
                    .expect("benchmark policies must compose")
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_read_restrictions);
criterion_main!(benches);
