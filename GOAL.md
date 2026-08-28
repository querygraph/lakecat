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
  `querygraph/catalog-bench@1f2014e` contains the Phase 1 publication plus the
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
- The remaining Phase 2 implementation has the common engine-neutral evidence
  boundary and source-bound Flink execution path committed.
  Trino 483 has a closed policy, renderer, server configuration, state machine,
  bounded decoders, process invocation boundary, and private configuration
  staging and supervised stock-launcher lifecycle with typed readiness and
  process-group cleanup; the next unit must implement its concrete effects and
  run the same no-shim workflow. No Flink or Trino production interoperability claim
  exists until its complete optimized evidence is materialized and validated.
  The active Phase 2 front is now fresh Flink materialization/publication,
  followed by concrete Trino effects and the largest honest DuckDB path.
- Phase 1 is closed. All C1-01 through C1-10 units and exit gates have exact
  committed evidence. The active delivery front is Phase 2 multi-engine
  interoperability.

## Next Stage

1. Complete the remaining Phase 2 work for Flink, Trino, and the largest honest DuckDB path;
   publish only independently admitted optimized evidence and correlate
   OpenLineage only where the pinned engine integration can prove it.
2. Execute the remaining catalog-community phases in dependency order: failure
   and recovery plus migration/federation; the Apache Ossie foundation in its
   owning repositories; the TPC-DS semantic supply chain; and evidence-linked
   upstream/community release artifacts. Treat every phase's exit criteria and
   global acceptance gates in `docs/catalog-community/` as mandatory.
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
