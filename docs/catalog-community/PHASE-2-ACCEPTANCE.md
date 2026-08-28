# Catalog Community Phase 2 Acceptance Ledger

Phase 2, **multi-engine interoperability**, is complete. The neutral harness
publishes four independently reviewed, no-shim correctness bundles covering
stock Spark, Flink, Trino, and DuckDB against LakeCat, Polaris, Gravitino, and
Lakekeeper. This ledger does not turn those correctness results into a
performance ranking and does not infer engine-native lineage from catalog
state.

## Accepted stock-engine evidence

| Engine | Pinned client | Accepted catalog-bench revision | Fresh run | Immutable bundle |
| --- | --- | --- | --- | --- |
| Spark | Spark 4.1.3, Iceberg 1.11.0 | `1f2014e` | `sparkv2_08280548` | `results/v1/spark-v2-65f0a4c3-2026-08-28/` |
| Flink | Flink 2.1.3, Iceberg 1.11.0, Hadoop 3.4.3 | `c375892` | `flinkv2_08280635` | `results/v1/flink-v2-65f0a4c3-2026-08-28/` |
| Trino | Trino 483, Iceberg 1.11.0 | `6886c35` | `trino_0828f26c` | `results/v1/trino-v2-b424f778-2026-08-28/` |
| DuckDB | DuckDB 1.5.3 and official signed Iceberg/httpfs/Avro extensions | `e9febf6` | `duckdb_0828h` | `results/v1/duckdb-v2-b8be6bc9-2026-08-28/` |

Every row uses the common v2 write/read/additive-evolution contract through
protocol-native Iceberg REST bindings and shared MinIO. Each of the four
catalogs passes for each engine. The bundle results are unranked correctness
claims; they contain no eligible performance measurements.

The final DuckDB path pins LakeCat `b8be6bc9` and Sail `54217703`. The latter
contains the reusable REST update decoding and partition-spec application
repairs exposed by the stock client. The complete publication boundary is
`querygraph/catalog-bench@e9febf669c22ab77e728dcf212b86932b6d978fd`.

## C2-07 — admitted OpenLineage correlation

The supported correlation boundary is LakeCat's governed Sail-planned read and
the QueryGraph/QGLake handoff. On 2026-08-28, at LakeCat `92385a58`, QueryGraph
`32c28c4`, and Sail `54217703`, the following stable-toolchain gate passed:

```sh
scripts/qglake-handoff-local.sh
```

The gate started LakeCat with its Turso catalog spine and Grust Turso graph
sink, created the physical Iceberg fixture, planned and fetched the governed
read through Sail, drained the transactional lineage/outbox boundary, replayed
the saved artifacts in LakeCat, and ran QueryGraph's locked `lakecat-verify`
and `lakecat-import` commands. LakeCat then strictly self-verified the saved
handoff summary.

Accepted observations from
`target/qglake-handoff/handoff-summary.json`:

| Proof | Observed value |
| --- | --- |
| Handoff status | `verified` |
| Delivered outbox/lineage events | 26 |
| OpenLineage aggregate hash | `sha256:2f6c78d138d2ee3130a36528f00eb4068e6327f82d8aca20439ef009514487b6` |
| Governed plan OpenLineage hash | `sha256:9a8659d6216fecaba96bf81ba8ea78bb06f56c4b3b8ec4343a5d43ebb64ee625` |
| Governed task-fetch OpenLineage hash | `sha256:22ac1cc1faef2c574dd89bd3d04b29b7f0e8479da90167930de644547b366623` |
| Bundle semantic hash | `sha256:1242aa191647582a99228e99a477770a46ee7a62df0b31aed15335f8e26e00d1` |
| Graph semantic hash | `sha256:2c32eaec43a9043c4a764e749afb851f68a59efcb471790ff9126fef5b8010ed` |
| QueryGraph import hash | `sha256:6ace38202db2e833507a1929a370e4709a130db00b922590f3afca6b2059eb40` |
| QueryGraph verification/import agreement | one table, one view, identical bundle/graph/OpenLineage/import hashes |

The accepted event vocabulary includes `table.scan-planned` and
`table.scan-tasks-fetched`, with the policy-narrowed projection, row predicate,
requested/effective statistics fields, principal, and authorization receipts
bound into replay evidence. This proves that the supported QG-stack read path
correlates admitted catalog work with OpenLineage and rejects a mismatched saved
handoff.

### Explicit non-claims

The stock Spark, Flink, Trino, and DuckDB benchmark bundles do not contain an
engine-native OpenLineage emitter or an admitted engine run-event identity.
Therefore this phase does **not** claim that their catalog calls are correlated
to engine-native OpenLineage jobs. Adding such a claim requires a separately
pinned integration that emits a run identity and survives the same artifact,
schema, replay, and hash checks. Catalog request timing, table state, or object
creation alone is not lineage evidence.

## Exit decision

C2-01 through C2-08 are complete:

- one engine-neutral, no-shim workflow is implemented and published;
- all four stock engines pass it against all four scoped catalogs;
- raw/result bundles and the generated publication index are immutable and
  independently reviewed;
- the supported governed Sail/QGLake path has admitted, replayable OpenLineage
  evidence; and
- unsupported engine-native correlation is named as a non-claim.

Phase 3 failure, recovery, migration, and federation is now the active delivery
front.
