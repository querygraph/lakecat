# LakeCat Goal

This is the live post-foundation goal. The original LakeCat charter is preserved
unchanged in `docs/completed/GOAL1.md`.

## Objective

Prepare and sustain LakeCat as a release-ready Rust-native, Iceberg REST
catalog foundation for QueryGraph. Keep the catalog boundary thin, preserve
standard client compatibility, and move reusable table semantics to Sail,
graph behavior to Grust, and governance semantics to TypeSec.

## Current Stage

- The latest release is v0.3.0 (Ocelot — stock-client Iceberg REST
  conformance, FABLE-REVIEW-1, Grust/TypeSec 0.12); see `RELEASE.md`,
  `STATUS.md`, `docs/RELEASES.md`, and `CHANGELOG.md` for the recorded
  release-candidate proof.
- Do not rebuild tracked book artifacts unless deliberately finishing a release.
  Keep `docs/book/lakecat.md` current as behavior and workflows change.
- Keep CI manual-only. Local release evidence is authoritative.
- Current LakeCat dependencies are the published Grust `0.12.0` (Lobster) and
  TypeSec `0.12.0` (Torcello) crates, plus Sail as a Cargo git dependency on
  `querygraph/sail#lakecat` (see `LAKECAT-SAIL.md`). QueryGraph's live `qg-rust`
  importer matches LakeCat's receipt-chain contract; refresh its stale
  dependency-guide examples before QueryGraph's next public release.
- The catalog-community program is active on
  `codex/catalog-community-phase-1`. Phase 0 is closed. LakeCat's public
  acceptance ledger is synchronized through Phase 1. The neutral
  `querygraph/catalog-bench@e9febf6` contains the Phase 1 publication plus the
  active Phase 2 implementation. Its Phase 1 evidence remains the reviewed immutable
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
- Phase 1 is closed. All C1-01 through C1-10 units and exit gates have exact
  committed evidence. With Phase 2 multi-engine interoperability complete, the
  active delivery front is Phase 3 failure, recovery, migration, and federation.
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

## Next Stage

1. Execute the remaining catalog-community phases in dependency order: failure
   and recovery plus migration/federation; the Apache Ossie foundation in its
   owning repositories; the TPC-DS semantic supply chain; and evidence-linked
   upstream/community release artifacts. Treat every phase's exit criteria and
   global acceptance gates in `docs/catalog-community/` as mandatory.
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
