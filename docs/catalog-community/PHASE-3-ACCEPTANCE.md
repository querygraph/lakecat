# Catalog Community Phase 3 Acceptance Ledger

This ledger accumulates evidence for Phase 3, **failure, recovery, and
migration**. A completed row proves only its named backlog unit. The phase does
not close until C3-01 through C3-06 and the Phase 3 exit gates are complete.

## C3-01 — deterministic network and object-store faults

Accepted catalog-bench revision:

- `catalog-bench@1633d30` publishes the deterministic proxy, isolated Compose
  overlay, neutral scenario, runnable Linux ARM64 profile, one-command fresh
  runner, reviewed source evidence, and reproduction guide.

The proxy has distinct `before-upstream` and `after-upstream` disconnect
phases, occurrence matching, and a bounded injection count that keeps automatic
client retries observable. Its control and evidence schemas reject unknown or
invalid rules. Evidence retains the method, rule identity, phase, match number,
upstream status when one exists, and a path hash; it does not retain headers,
queries, bodies, signed URLs, credentials, or raw metadata paths.

Fresh run `objfault_0828a` used a signed S3 metadata PUT and direct MinIO state
observation:

| Case | Client | Proxy observation | Direct object state |
| --- | --- | --- | --- |
| Before upstream | disconnected | no upstream status | absent |
| After upstream | disconnected | HTTP 200 | present |

Both cases used content hash
`sha256:9c6a95372144d03bb2f58ebf9dc3049576560f9b6811787e3eb3baec95f02f61`.
The reviewed summary hash is
`sha256:350c5a2e9d2c61b6e7b19c51bc0d306d5a24fecd8f0fc1259265caee14cb3faf`.
The runner removed both objects, every run container, and every run volume.

The exact accepted command was:

```sh
docker/run-object-faults.sh objfault_0828a
```

Static and contract verification included:

```sh
(cd docker/minio/tools && \
  test -z "$(gofmt -l .)" && \
  go mod tidy -diff && \
  go vet ./... && \
  go test ./...)

cargo fmt --all -- --check
cargo test -p catalog-bench-common --locked
cargo run -p catalog-bench-contract --locked -- validate \
  profiles/v1/object-faults-2026-08-28.json \
  scenarios/v1/object-store.metadata-persistence-faults.json
docker compose -f docker-compose.yml -f docker-compose.fault.yml \
  --profile lakekeeper --profile polaris --profile gravitino config --quiet
git diff --check
```

The overlay provides one object-store proxy and one REST proxy for LakeCat,
Polaris, Gravitino, and Lakekeeper without changing the already-published
correctness topology. This closes the injection substrate, not catalog recovery
or performance. C3-02 and C3-04 own the catalog-specific behavior.

## C3-02 — commit ambiguity, exact retry, and restart recovery

Accepted catalog-bench revision:

- `catalog-bench@c7eb664` publishes the v2 standard Iceberg REST recovery
  scenario, deterministic in-flight request gate, fresh four-catalog runner,
  raw sanitized `restart_0828d` evidence, review record, and reproduction guide.

Every catalog passed both accepted-state ambiguity cases. A disconnect before
upstream admission left the property absent and exact retry returned 200. A
disconnect after upstream HTTP 200 left the property accepted; exact retry was
safe and the accepted property remained unchanged.

For restart, the proxy transmitted one request-body byte, recorded the
sanitized `during-upstream` event, paused the remainder, restarted only the
target catalog service, and released the old connection. Each interrupted
request returned HTTP 502 and no partial property mutation was observed.

| Catalog | Fixture after restart | Exact retry | Result |
| --- | --- | --- | --- |
| LakeCat | present | HTTP 200 | pass |
| Polaris | absent | HTTP 500 | fail in the benchmark's ephemeral persistence configuration |
| Gravitino | present | HTTP 200 | pass |
| Lakekeeper | present | HTTP 200 | pass |

Lakekeeper alone advertised idempotency. Its exact replay returned 200. A
same-key/different-content request also returned cached 200 without mutating
state; this is retained as a content-binding defect and is not described as
exactly-once behavior. Polaris's result is scoped to the benchmark's ephemeral
server configuration and does not claim behavior for an external database.

The reviewed summary hash is
`sha256:7083e202a2a4a1ad6d44faf01e51684b3a0c028d4b1ce538c643736b8cad76dc`.
Per-catalog artifact hashes are:

- LakeCat: `sha256:632b7352472dfaa3d126ecd17b7ba6c2635ad83fff8d47cfb9f123d7f01d5b23`
- Polaris: `sha256:61bfab91797cdb7d1ced131259766502f8dd5d491e63aa0c8082e1fea2442930`
- Gravitino: `sha256:5556685c8572468359258c86db83acf950e02fed62d18be5c6c1540dec6387f7`
- Lakekeeper: `sha256:3ce4f86e612d97dfdd89c3594047bb7d2358bfc357efd72b261e4f403b716739`

The runner removed every run container and volume. This closes C3-02 only; it
does not prove state-store/outbox failure, rolling restart, backup/restore,
migration, federation, or performance.

## C3-03 — state failure and outbox outage recovery

Accepted LakeCat revision:

- `lakecat@b6336c54` derives every lineage and OpenLineage event time from the
  durable outbox admission timestamp instead of retry time and adds a complete
  outage/backlog/retry acknowledgement proof.

The state-store failure proof forces the transactional Turso outbox insert to
fail after the paired audit insert is attempted. The transaction rolls back:
neither audit nor outbox state is admitted. The sink-outage proof admits exactly
one `table.created` audit/outbox pair, fails lineage projection after graph
projection, and verifies all of the following:

- the drain returns failure and acknowledges no event;
- exactly one pending event remains, with the original durable event ID;
- retry emits byte-equivalent lineage input, including the same admission time;
- graph replay uses the same stable event IDs, allowing sink-owned idempotency;
- successful acknowledgement reports exactly one delivered event; and
- the pending backlog is empty afterward.

Verification passed on the stable toolchain:

```sh
cargo test -p lakecat-store --features turso-local \
  turso_store_rolls_back_audit_when_outbox_insert_fails -- --test-threads=1
cargo test -p lakecat-service \
  outbox_sink_outage_replays_exact_admitted_event_and_clears_backlog \
  -- --test-threads=1
cargo test -p lakecat-service --all-features -- --test-threads=1
```

The all-features service gate passed 492 library tests and five configured
integration tests. Source hashes are:

- outbox projector: `sha256:7e2a311f6e04fd2f4cecba4eb196431a91354558acf67dfb3d83e59626522468`
- outage/replay tests: `sha256:04718439019e711876f2725ba7a18c2bdc08d088f91863b0e2ff37d1c85fac4b`
- Turso failure tests: `sha256:c8959bb2d0817e188378c736c64a39e8f6ac1439e0cb4dacec8502602ae90698`

This is at-least-once projection with exact stable replay identity. Downstream
sinks remain responsible for idempotency by event ID; the proof does not claim
distributed exactly-once delivery. It closes C3-03, not backup/restore,
rolling restart, migration, or federation.

## C3-04 — per-catalog restart and cold restore

Accepted catalog-bench revision:

- `catalog-bench@2debd3f` publishes the neutral cold backup/restore scenario,
  ownership-preserving source-built archive helper, fresh runner, sanitized
  before/after identities, archive hashes, cleanup proof, and reproduction guide.

The restart half is the independently targeted service restart from C3-02:
each catalog process restarts while the shared object store and other catalogs
remain available. The cold-restore half creates a standard Iceberg REST fixture,
stops the state owner, archives its run-owned durable volume, deletes and
recreates that volume, restores it, and compares table UUID plus
metadata-location hash through standard REST.

Fresh run `backup_0828a` produced reviewed summary hash
`sha256:592e122218671efc08ea7d1bed726ff40677db3454bb24422a9516739efdea0a`:

| Catalog | State | Cold restore |
| --- | --- | --- |
| LakeCat | Turso | pass |
| Polaris | ephemeral benchmark state | fail: fixture absent (HTTP 404) |
| Gravitino | SQLite | pass |
| Lakekeeper | PostgreSQL | pass |

Archive evidence is retained by size and SHA-256 rather than committing
database bytes. The LakeCat archive is 4,959 bytes with hash
`cd7b1097bbe3b302ff83076d7eb6fb7a0f742a159e0686a4eb6fd717a14c33a1`;
Gravitino is 699 bytes with hash
`b51980245e36c9b5a65e4b4c49d21faf25025dc7939b8697591111a595a26853`;
Lakekeeper is 7,898,765 bytes with hash
`6fa0c37829952cc9d3283ca744d56d7111d2e9f70a23440636ce9507b6a29f7b`.

The runner removed every project container and volume. Polaris's failure is
scoped to the no-volume benchmark topology. This is cold byte-level recovery,
not vendor-supported logical backup, online backup, point-in-time recovery,
rolling upgrade, or a disaster-recovery SLA.

## C3-05 — QueryGraph catalog migration and federation semantics

Accepted revisions:

- `querygraph@9ef2e21` defines the transport-neutral Iceberg semantic identity
  verifier and its optional stock-PyIceberg live harness.
- `sail@65df4aa4` exposes bounded standard metadata decompression and separates
  gzip suffix recognition from metadata-version discovery.
- `lakecat@f09f7896` consumes that Sail boundary for registration while retaining
  location controls and the 64 MiB encoded/decoded metadata limit.
- `catalog-bench@74e098f` publishes the fresh runner and reviewed evidence.

Fresh run `migrate_0828d` used one isolated Compose project, shared MinIO, the
source-built LakeCat commit, Polaris 1.7.0, Lakekeeper 0.13.3, and pinned stock
PyIceberg 0.11.1. Each direction creates three rows and a non-empty snapshot,
registers the exact metadata pointer in the destination through standard REST,
then independently loads and scans both sides:

| Direction | Semantic mismatches | Snapshots / refs | Exact data |
| --- | ---: | ---: | --- |
| LakeCat → Polaris | 0 | 1 / 1 | 3 rows, matching digest |
| Polaris → LakeCat | 0 | 1 / 1 | 3 rows, matching digest |
| LakeCat → Lakekeeper | 0 | 1 / 1 | 3 rows, matching digest |
| Lakekeeper → LakeCat | 0 | 1 / 1 | 3 rows, matching digest |

The semantic comparison covers table UUID, format version, all schemas and the
current schema, all partition specs and the default spec, all sort orders and
the default order, snapshots and current snapshot, refs, and metadata location.
Every exact data digest is
`sha256:47667d3010f5b9a4f6d9c3f26873938dc46adecf8392d7ba1d1b88409f18315e`.
The raw summary hash is
`sha256:7d1f426db69b70d718b7b946ea532d75e3642451e897eed6374ac30220db9e38`;
its canonical content hash is
`sha256:be68dda139534b1af64fd71efd694496fe57500216b89b3908e016a98e96c0c9`.

Focused verification passed 58 QueryGraph Python tests, seven Sail metadata
loader tests, 493 LakeCat service tests plus five configured binary tests, the
LakeCat local-dependency contract, JSON/evidence equivalence checks, and fresh
project cleanup with zero remaining containers or volumes.

This proves portable metadata-pointer federation and migration for the named
catalog pairs against a shared object store. It does not prove physical object
copy, cross-cloud transfer, concurrent dual-writer safety, incremental sync,
cutover orchestration, or the legacy Hive/Hadoop/Glue path owned by C3-06.

## C3-06 — legacy HadoopCatalog migration

Accepted revisions:

- `querygraph@b176d1d2` provides a stock Spark HadoopCatalog fixture, semantic
  verifier, focused tests, and the operator cookbook
  `docs/catalog-community/HADOOP-TO-LAKECAT.md`.
- `lakecat@20116489` compares parsed metadata URL components and admits
  Hadoop's equivalent `file:/` spelling only for children of the configured
  `file:///` storage root.
- `catalog-bench@ad3707d` publishes the reviewed `hadoop_0828i` evidence;
  `catalog-bench@98c2a3e` advances the deployment provenance assertion to that
  exact LakeCat source revision.

Fresh run `hadoop_0828i` used stock Spark 4.1.3 and Iceberg 1.11.0. Spark
created the source in a standard HadoopCatalog on a run-owned shared filesystem,
performed additive schema evolution, wrote two snapshots under two partition
specifications, and created an audit branch. It then registered the exact
metadata pointer through LakeCat's standard Iceberg REST endpoint. Independent
loads of source and destination reported zero semantic mismatches:

| Property | HadoopCatalog source | LakeCat destination |
| --- | ---: | ---: |
| Snapshots | 2 | 2 |
| Partition specs | 2 | 2 |
| Refs | 2 | 2 |
| Rows | 3 | 3 |

Both semantic digests are
`sha256:a0f7d2bd253157da45ec523ddd49398ff4f579a1ac5410f4d410b25d6976ccca`.
Both exact data digests are
`sha256:399e7ecbc386cd9c12454862d2cfdd2da5f20c6fe264396215b7110212258b50`.
The metadata-location digest is
`sha256:81239a07d208c8ec5f9facae38f0c8c4da39b68c1841b061f7943e24e2ee3a47`.
The sanitized summary's canonical content hash is
`sha256:7b45bced25c8dd7966b74b88df2ed3d867f2e4738efe7aacc10ce1cedae416e4`.

The accepted command was:

```sh
docker/run-querygraph-hadoop-migration.sh hadoop_0828i
```

The runner rejects non-fresh state, binds QueryGraph and LakeCat to exact
revisions, retains only a digest of the metadata location, and removed every
run container and volume. QueryGraph's focused tests passed; LakeCat's
all-feature service suite passed 494 library and five binary tests; and the
complete locked catalog-bench workspace passed.

This is a same-filesystem legacy HadoopCatalog registration cookbook. It does
not claim Hive Metastore or Glue coverage, physical object copying, cross-cloud
transfer, concurrent dual-writer safety, incremental synchronization, or
automated cutover.

## Phase 3 exit gates

Phase 3 is closed. C3-01 through C3-06 are complete and collectively provide:

- deterministic, source-pinned, occurrence-bounded fault injection;
- visible catalog-specific ambiguity, retry, restart, and cold-restore results;
- transactional catalog/outbox failure and at-least-once replay proof;
- peer-catalog and legacy HadoopCatalog migration with semantic and data checks;
- exact source revisions, runner/config hashes, sanitized reviewed evidence,
  explicit non-claims, and fresh-state guards; and
- successful cleanup with no run-owned containers or volumes remaining.

The implementations remain in their owning repositories: the neutral lab in
catalog-bench, migration composition in QueryGraph, Iceberg decoding in Sail,
and only catalog admission/location enforcement in LakeCat. No Phase 3 result
is ranked as performance evidence. Phase 4 Apache Ossie work may now begin.
