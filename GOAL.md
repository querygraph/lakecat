# LakeCat Goal

This is the live post-foundation goal. The original LakeCat charter is preserved
unchanged in `docs/completed/GOAL1.md`.

## Objective

Prepare and sustain LakeCat as a release-ready Rust-native, Iceberg REST
catalog foundation for QueryGraph. Keep the catalog boundary thin, preserve
standard client compatibility, and move reusable table semantics to Sail,
graph behavior to Grust, and governance semantics to TypeSec.

## Current Stage

- The latest release is v0.4.0 (Caracal — the catalog-community release,
  governed-scan grants, Grust 0.13/TypeSec 0.14); see `RELEASE.md`,
  `STATUS.md`, `docs/RELEASES.md`, and `CHANGELOG.md` for the recorded
  release-candidate proof.
- Do not rebuild tracked book artifacts unless deliberately finishing a release.
  Keep `docs/book/lakecat.md` current as behavior and workflows change.
- Keep CI manual-only. Local release evidence is authoritative.
- Current LakeCat dependencies are the published Grust `0.13.0` (Prawn) and
  TypeSec `0.14.0` (Dorsoduro) crates, plus Sail as a Cargo git dependency on
  `querygraph/sail#lakecat` (see `LAKECAT-SAIL.md`). QueryGraph's live `qg-rust`
  importer matches LakeCat's receipt-chain contract; refresh its stale
  dependency-guide examples before QueryGraph's next public release.
- The catalog-community program is active on
  `codex/catalog-community-phase-1`. Phase 0 is closed. LakeCat's public
  acceptance ledger is synchronized through Phase 1. The neutral
  `querygraph/catalog-bench@74e098f` contains the current Phase 3 migration
  publication. Its Phase 1 evidence remains the reviewed immutable
  five-scenario by five-catalog correctness bundle (20 pass, five fail), the
  production contention bundle, one-command smoke/full recomputation,
  generated known gaps, and a bundle-wide secret scan. LakeCat C1-10 is closed
  at `0dee124b`; Phase 1 is complete without converting correctness evidence
  into a performance claim.
- Phase 2 Spark delivery is complete in `querygraph/catalog-bench@1f2014e`.
  Fresh run `sparkv2_08280548` used stock Spark 4.1.3 and Iceberg 1.11.0
  against LakeCat `65f0a4c3`, Polaris 1.7.0, Gravitino 1.3.0, and Lakekeeper
  0.13.3 through protocol-native REST bindings and shared MinIO. All four pass
  all 14 required write/read/evolution, independent-state, sanitization, and
  cleanup assertions. The reviewed transcripts and deterministic immutable
  bundle are published under
  `results/v1/spark-v2-65f0a4c3-2026-08-28/`; this is an unranked correctness
  matrix, not a performance claim. LakeCat `5d62f1c4` fixed catalog-owned
  create-table field IDs, and `65f0a4c3` completed Iceberg REST multipart
  namespace handling required by Spark metadata-table fallback.
- Phase 2 Flink delivery is complete in `querygraph/catalog-bench@c375892`.
  Fresh run `flinkv2_08280635` used stock Flink 2.1.3, Iceberg 1.11.0, and the
  checksum-locked Hadoop 3.4.3 client runtime against the same four catalogs
  and shared MinIO. All four pass the complete v2 write/read/evolution,
  independent-state, sanitization, and cleanup contract. The reviewed,
  deterministic, unranked correctness bundle is published under
  `results/v1/flink-v2-65f0a4c3-2026-08-28/`. The source-bound runner selects
  Flink from the profile, admits every copied runtime byte, and uses Flink's
  stock `local` target for the one-shot isolated execution topology.
- Phase 2 Trino delivery is complete in `querygraph/catalog-bench@6886c35`.
  Fresh run `trino_0828f26c` used stock Trino 483 and Iceberg 1.11.0 against
  LakeCat `b424f778`, Polaris 1.7.0, Gravitino 1.3.0, and Lakekeeper 0.13.3.
  All four pass the common 14-assertion write/read/evolution, independent-state,
  sanitization, and cleanup contract. The reviewed deterministic bundle is
  published under `results/v1/trino-v2-b424f778-2026-08-28/`. LakeCat's
  configured warehouse root supports standard REST create requests without an
  explicit table location; catalog-bench now reconciles engine-relative
  snapshot baselines and bounded standard gzip Iceberg metadata. This remains
  an unranked correctness result.
- Phase 2 DuckDB delivery is complete in `querygraph/catalog-bench@e9febf6`.
  Fresh run `duckdb_0828h` used stock DuckDB 1.5.3 with its official signed
  Iceberg, HTTPFS, and Avro extensions against LakeCat `b8be6bc9`, Polaris
  1.7.0, Gravitino 1.3.0, and Lakekeeper 0.13.3. All four pass the common
  independently validated write/read/evolution and cleanup contract. The
  reviewed deterministic bundle is published under
  `results/v1/duckdb-v2-b8be6bc9-2026-08-28/`. DuckDB exposed and drove fixes
  for staged Iceberg table creation in LakeCat and spec-correct REST update
  decoding plus `add-spec` application in Sail `54217703`; no catalog-specific
  client shim was introduced. This remains an unranked correctness result.
- Phase 2 is closed. Its common engine-neutral evidence boundary and the
  published Spark, Flink, Trino, and DuckDB paths are complete. The governed
  Sail-planned QGLake path delivered 26 admitted lineage events and LakeCat plus
  QueryGraph independently verified the same OpenLineage aggregate hash; exact
  evidence is in `docs/catalog-community/PHASE-2-ACCEPTANCE.md`. The stock
  engine bundles do not prove engine-native OpenLineage and make no such claim.
- Phase 1 and Phase 2 are closed with exact committed evidence; their detailed
  acceptance ledgers remain the authoritative proof.
- Phase 3 C3-01 is complete at `querygraph/catalog-bench@1633d30`. Fresh run
  `objfault_0828a` proves a signed metadata PUT disconnected before upstream
  leaves no object, while an upstream HTTP 200 whose response is disconnected
  leaves the object present. The source-pinned proxy, isolated four-catalog
  overlay, runnable profile, exact hashes, cleanup, and non-claims are recorded
  in `docs/catalog-community/PHASE-3-ACCEPTANCE.md`. Catalog recovery remains
  open in C3-03 through C3-06.
- Phase 3 C3-02 is complete at `querygraph/catalog-bench@c7eb664`. Fresh run
  `restart_0828d` proves before/after response-loss reconciliation and a real
  mid-request service restart across LakeCat, Polaris, Gravitino, and
  Lakekeeper. LakeCat, Gravitino, and Lakekeeper preserve the fixture and
  accept exact retry; the benchmark's ephemeral Polaris configuration loses
  it. Lakekeeper's advertised idempotency remains non-content-bound. Exact
  artifact hashes, cleanup proof, configuration scope, and non-claims are in
  `docs/catalog-community/PHASE-3-ACCEPTANCE.md`.
- Phase 3 C3-03 is complete at `lakecat@b6336c54`. Turso rolls back the paired
  audit/outbox write when durable admission fails. A real sink-outage test
  retains one pending event, replays identical lineage input and stable graph
  event IDs, acknowledges it once after recovery, and empties the backlog.
  This is an explicit at-least-once projection contract, not a distributed
  exactly-once claim.
- Phase 3 C3-04 is complete at `querygraph/catalog-bench@2debd3f`. Targeted
  restart evidence is paired with a fresh cold restore that deletes and
  recreates run-owned state volumes. LakeCat/Turso, Gravitino/SQLite, and
  Lakekeeper/PostgreSQL preserve exact table identity; the no-volume Polaris
  topology loses it. The proof is not an online-backup or disaster-recovery SLA.
- Phase 3 C3-05 is complete at `querygraph/catalog-bench@74e098f`, QueryGraph
  `9ef2e21`, LakeCat `f09f7896`, and Sail `65df4aa4`. Fresh stock-PyIceberg
  migration in both directions between LakeCat and Polaris and between LakeCat
  and Lakekeeper preserves all compared Iceberg semantics, a non-empty snapshot
  and ref, the exact metadata pointer, and an exact three-row scan. The
  Lakekeeper path exposed and fixed bounded gzip metadata-pointer registration.
  Physical copying, dual-writer federation, and legacy catalog migration remain
  outside this proof.
- Phase 3 C3-06 is complete at QueryGraph `b176d1d2`,
  `querygraph/catalog-bench@ad3707d`, and LakeCat `20116489`. Fresh run
  `hadoop_0828i` uses stock Spark 4.1.3 and Iceberg 1.11 HadoopCatalog, evolves
  two snapshots and two partition specs, retains two refs, and registers the
  exact metadata pointer in LakeCat. Both sides independently scan the same
  three rows. The run is isolated, sanitized, reproducible, and leaves no
  containers or volumes.
- Phase 3 is closed. C3-01 through C3-06 cover deterministic storage/network
  faults, ambiguous commits and restarts, transactional outbox recovery, cold
  state restoration, peer REST metadata-pointer migration, and a legacy
  HadoopCatalog cookbook. The exact proofs and explicit non-claims are in
  `docs/catalog-community/PHASE-3-ACCEPTANCE.md`.
- Phase 4 is closed at QueryGraph `5177c2e`, Sail `9f6f8065`, LakeCat
  `d7b9e3be`, TypeSec `3c5e0b1`, and Grust `cec8ce1`. The pinned upstream
  Ossie artifacts validate and round-trip losslessly; physical validation,
  durable publication CAS/outbox state, signed semantic decisions, stable graph
  taxonomy, and fail-closed admission ordering remain in their owning repos.
  Exact evidence is in `docs/catalog-community/PHASE-4-ACCEPTANCE.md`. Phase 5
  TPC-DS was its accepted successor.
- Phase 5 is closed at LakeCat `8917e5c6`, QueryGraph `f0e4afd`, Grust
  `e5edc99`, and `querygraph/catalog-bench@61183ba`. Fresh stock-Spark run
  `tpcds_0828g` creates the physical fixtures, policy-binds and CAS-publishes
  the exact pinned Ossie model, drains graph/OpenLineage replay, evaluates five
  representative answers, binds seven proof bases, and rejects all six required
  drift dimensions. The upstream Polaris converter’s live TPC-DS run is
  explicitly `verified-with-loss`, not described as lossless.
- Phase 6 is closed at QueryGraph `f0e4afd` and
  `querygraph/catalog-bench@285415d`. The public Q3 report, immutable evidence
  index, reproduction and demo guides, Ossie report-contract proposal, feedback
  backlog v2, and review opportunities for LakeCat #4, Polaris #5403, Gravitino
  #12719, and Lakekeeper #2002 are published. No maintainer endorsement is
  inferred from an open issue.

## Next Stage

1. Maintain the completed catalog-community evidence as immutable history.
   Triage public maintainer feedback into backlog v3 with source URLs and new
   artifact versions; do not rewrite accepted Q3 evidence.
2. Extend OpenLineage correlation to a stock engine only when a separately
   pinned emitter proves engine run identity through the same admission and
   replay boundary.
3. Keep release proof fresh after executable changes with the full local gate.
4. Replace temporary Sail helper bridges only when upstream helpers are
   published and covered by Sail tests.
5. Keep v4 JSON bridging explicit. Apache Iceberg v4 remains a draft; typed
   metadata, relative-location, manifest, delete, and planning support belongs
   in Sail after formal specification adoption.
6. Keep QueryGraph QGLake verify/import as the end-to-end acceptance boundary.
7. Continue to use the repo boundaries and verification discipline in
   `AGENTS.md` as binding guidance.

## Source Of Truth

Read `AGENTS.md`, `DESIGN.md`, `STATUS.md`, `ARCHITECTURE.md`, `RELEASE.md`,
the LakeCat book, and the live code before selecting work. Historical goals and
OPUS documents under `docs/completed/` are audit records, not active plans.
