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
  acceptance ledger is synchronized through C1-06, while the neutral
  `querygraph/catalog-bench` branch has also implemented and documented C1-07
  stock PyIceberg interoperability, C1-08 contention execution, and the
  production-contention publication portion of C1-09. Those later units must be
  reconciled into LakeCat's backlog and acceptance records before Phase 1 is
  declared complete.
- Phase 2 implementation has started in `querygraph/catalog-bench`: the common
  stock-engine contract, source-bound Spark 4.1.3/Iceberg 1.11.0 runtime, fresh
  four-catalog Spark workflow, reviewed result materializer, engine-neutral
  evidence boundary, and source-bound Flink execution path are committed.
  Trino 483 has a closed policy, renderer, server configuration, state machine,
  bounded decoders, and process invocation boundary; the next unit must finish
  private configuration staging and concrete lifecycle supervision, then run
  the same no-shim workflow. No Flink or Trino production interoperability claim
  exists until its complete optimized evidence is materialized and validated.
- Phase 1 remains open only on C1-09. C1-10 is implemented: LakeCat uses a
  versioned component-safe namespace key across namespace, table, view/receipt,
  soft-delete, and policy scope, and Turso atomically migrates validated legacy
  rows with idempotent marking and fail-closed rollback. C1-09 still needs its
  complete one-command smoke/full publication contract, generated known-gaps
  surface, bundle-wide secret scan, and final Phase 1 exit rerun.

## Next Stage

1. Recover and finish the interrupted `catalog-bench` Trino configuration and
   lifecycle unit without discarding its existing working-tree changes; update
   its changelog, pass focused and repository gates, then commit and push it as
   an independent unit.
2. Reconcile C1-07, C1-08, and completed C1-09 evidence into LakeCat's public
   backlog, status, acceptance ledger, and reader documentation without copying
   neutral raw evidence into LakeCat.
3. Close every Phase 1 exit criterion, including one-command smoke/full
   profiles, immutable generated reports, known gaps, and secret scanning.
4. Complete Phase 2 for Spark, Flink, Trino, and the largest honest DuckDB path;
   publish only independently admitted optimized evidence and correlate
   OpenLineage only where the pinned engine integration can prove it.
5. Execute the remaining catalog-community phases in dependency order: failure
   and recovery plus migration/federation; the Apache Ossie foundation in its
   owning repositories; the TPC-DS semantic supply chain; and evidence-linked
   upstream/community release artifacts. Treat every phase's exit criteria and
   global acceptance gates in `docs/catalog-community/` as mandatory.
6. Keep release proof fresh after executable changes with the full local gate.
7. Replace temporary Sail helper bridges only when upstream helpers are
   published and covered by Sail tests.
8. Keep v4 JSON bridging explicit. Apache Iceberg v4 remains a draft; typed
   metadata, relative-location, manifest, delete, and planning support belongs
   in Sail after formal specification adoption.
9. Keep QueryGraph QGLake verify/import as the end-to-end acceptance boundary.
10. Continue to use the repo boundaries and verification discipline in
   `AGENTS.md` as binding guidance.

## Source Of Truth

Read `AGENTS.md`, `DESIGN.md`, `STATUS.md`, `ARCHITECTURE.md`, `RELEASE.md`,
the LakeCat book, and the live code before selecting work. Historical goals and
OPUS documents under `docs/completed/` are audit records, not active plans.
