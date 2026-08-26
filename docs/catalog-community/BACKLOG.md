# Catalog Community Delivery Backlog

Each unit is intended to be one reviewable commit or a deliberately small commit
series in one repository. `done` means the named acceptance evidence exists;
`blocked` requires a concrete external condition; `pending` does not imply support.
Phase transitions require the program's phase exit criteria and global
acceptance gates; they cannot be declared from this checklist alone.

## Phase 0 — baseline and design

| ID | Status | Owner | Unit and acceptance evidence |
| --- | --- | --- | --- |
| C0-01 | done | LakeCat | Read repository guidance and inventory LakeCat, catalog-bench, QueryGraph, Sail, Grust, and TypeSec at exact revisions. Evidence: `PHASE-0-BASELINE.md`. |
| C0-02 | done | LakeCat | Verify all three historical result hashes and independently read the aggregate ranking/error counts. Evidence: hashes and values in the baseline. |
| C0-03 | done with explained discrepancy | LakeCat | Attempt clean-checkout source and live Docker reproduction. LakeCat's locked source check passes; the historical catalog-bench ambient Sail defect and Docker VM exhaustion are documented exactly. The dependency defect is repaired by C0-09; no live timing is fabricated. |
| C0-04 | done | LakeCat | Verify current catalog, client, engine, object-store, and database versions from primary sources; select compatible Spark/Flink + Iceberg intersections. |
| C0-05 | done with test gap | LakeCat | Inspect pinned Apache Ossie schema, validator, TPC-DS example, Python models, converters, and Polaris boundary. Validator/TPC-DS and 9 Python tests pass; Maven test gap recorded. |
| C0-06 | done | LakeCat | Record neutral harness and Ossie ownership in `DESIGN.md` before implementation. |
| C0-07 | done | catalog-bench | Closed Rust ADTs, semantic validation, and generated Draft 2020-12 schemas landed in `afdb6b0`; schema equality is covered by integration tests and `schemas check`. |
| C0-08 | done | catalog-bench | Historical/current profiles, source/image/build provenance, explicit uncertainty, aggregate identity, and unresolved-artifact rejection landed through `6ff118e`. |
| C0-09 | done | catalog-bench | `1aed9f9` replaces all ambient Sail paths with immutable `querygraph/sail@bddb1706`; a detached clean worktree passed `cargo test --workspace --locked`. |
| C0-10 | done | catalog-bench | `7af1fb7` hash-verifies and recomputes all preserved TSV evidence into four aggregate records, an immutable manifest, and the generated pass-only concurrent matrix. |
| C0-11 | done | catalog-bench | `7af1fb7` adds document/bundle/import/matrix commands, separate integration and tamper tests, result/profile docs, and report migration; all strict workspace gates pass. |
| C0-12 | done | LakeCat | The book and program entry point now consume exact `catalog-bench@c0637076` evidence, remove the obsolete copied table/nonexistent repository, state the historical/live-rerun scope, and pass the unified full-book build. |
| C0-13 | done | both | Detached clean worktrees at `catalog-bench@c0637076` and `lakecat@1b0b9501` passed their full Phase 0 gates. Exact commands, counts, discrepancy treatment, and exit-criteria mapping are in `PHASE-0-ACCEPTANCE.md`; both branches are committed and pushed independently. |

## Phase 1 — Lakekeeper and behavioral conformance

| ID | Status | Owner | Unit and acceptance evidence |
| --- | --- | --- | --- |
| C1-01 | done | catalog-bench | `catalog-bench@dea3c575` adds the owned network, exact source-built MinIO, dedicated PostgreSQL 17.11 state, Lakekeeper 0.13.3 migration/bootstrap/warehouse/config gates, and typed drift-rejecting setup helpers. Fresh and repeat Docker proofs are recorded in `PHASE-1-ACCEPTANCE.md`. |
| C1-02 | done | catalog-bench | `catalog-bench@7269f885` adds schema-backed adapter ADTs, a 27-capability vocabulary, exact config/prefix/auth bindings for all five current catalogs, protocol-native versus disclosed-shim semantics, exhaustive coverage and endpoint/secret/drift validation, and comprehensive adapter documentation. All strict gates passed without changing historical profile or result bytes. |
| C1-03 | done | catalog-bench/LakeCat | `catalog-bench@feb803f8` implements strict config negotiation and endpoint-advertisement evidence; `catalog-bench@ec3e40d6` pins its source profile; `lakecat@09dd7ee3` corrects LakeCat's advertisement to the exact implemented Iceberg 1.11 routes. Optimized same-Docker probes, transcript hashes, sanitization checks, Polaris's proprietary-route failure, Nessie's config pass, and both repositories' strict gates are recorded in `PHASE-1-ACCEPTANCE.md`. The smoke transcripts are deliberately not publishable result bundles; C1-09 retains final materialization ownership. |
| C1-04 | done | catalog-bench/LakeCat | `catalog-bench@1f4e640` implements isolated namespace lifecycle, multipart hierarchy, bounded pagination, duplicate/missing-parent errors, optional properties, cleanup, and sanitized evidence; `catalog-bench@2c3ef82` pins provenance; `lakecat@42b2f34b` repairs every LakeCat failure. The optimized same-Docker five-catalog matrix and exact hashes are recorded in `PHASE-1-ACCEPTANCE.md` and `catalog-bench@b149ee74`. |
| C1-05 | pending | catalog-bench | Implement table create/list/load/register/rename/update/drop and spec-shaped error scenarios. |
| C1-06 | pending | catalog-bench | Implement commit-requirement, stale pointer, exact retry, and idempotency-drift scenarios. |
| C1-07 | pending | catalog-bench | Expand the stock PyIceberg workflow and classify unsupported operations explicitly for every catalog. |
| C1-08 | pending | catalog-bench | Add p50/p95/p99/max distributions, variance, cold start, readiness, and RSS capture with fixed resource limits. |
| C1-09 | pending | catalog-bench | Produce one-command smoke and full profiles, raw bundles, generated matrix, known-gaps page, and secret scan. |
| C1-10 | pending | LakeCat | Correct every remaining LakeCat conformance failure found; keep format fixes in Sail and policy/graph fixes in their owning repositories. C1-04 corrections are complete, while the pre-existing dot-joined namespace-key ambiguity requires one versioned component-safe migration across namespace-, table-, view-, and policy-derived keys. |

## Phase 2 — multi-engine interoperability

| ID | Status | Owner | Unit and acceptance evidence |
| --- | --- | --- | --- |
| C2-01 | pending | catalog-bench | Add Spark 3.5.9 + Iceberg 1.11.0 stock REST workflow. |
| C2-02 | pending | catalog-bench | Add Spark 4.1.3 + Iceberg 1.11.0 stock REST workflow and record the Spark 4.2 connector gap. |
| C2-03 | pending | catalog-bench | Add Flink 2.1.3 bounded/streaming workflows and record the Flink 2.3 connector gap. |
| C2-04 | pending | catalog-bench | Add Trino 483 read/write/evolution workflow. |
| C2-05 | pending | catalog-bench | Add DuckDB 1.5.3's largest honest REST/Iceberg workflow. |
| C2-06 | pending | catalog-bench | Run one common workflow against LakeCat, Polaris, Gravitino, and Lakekeeper with no undisclosed shims. |
| C2-07 | pending | LakeCat/QueryGraph | Correlate supported engine work with admitted OpenLineage evidence. |
| C2-08 | pending | catalog-bench | Publish the first evidence-generated interoperability report and raw bundle. |

## Phase 3 — failure, recovery, and migration

| ID | Status | Owner | Unit and acceptance evidence |
| --- | --- | --- | --- |
| C3-01 | pending | catalog-bench | Add deterministic network/object-store fault injection before and after metadata persistence. |
| C3-02 | pending | catalog-bench | Prove exact-request retry, idempotency-key drift, restart-during-commit, and accepted-state ambiguity behavior. |
| C3-03 | pending | LakeCat | Prove state-store failure and outbox sink outage/backlog/exact replay without invented or lost admitted events. |
| C3-04 | pending | catalog-bench | Add backup/restore and rolling restart evidence per catalog. |
| C3-05 | pending | QueryGraph | Implement semantics-preserving LakeCat↔Polaris and LakeCat↔Lakekeeper migration/federation verification. |
| C3-06 | pending | QueryGraph | Add one Hive, Hadoop, or Glue migration cookbook that verifies snapshots, schemas, specs, refs, metadata locations, and data reads. |

## Phase 4 — Apache Ossie foundation

| ID | Status | Owner | Unit and acceptance evidence |
| --- | --- | --- | --- |
| C4-01 | pending | QueryGraph | Vendor-by-hash or fetch-and-verify the pinned upstream schema/validator/examples without forking them. |
| C4-02 | pending | QueryGraph | Implement typed JSON/YAML import/export, multi-dialect and unknown-extension preservation, structural validation, and explicit loss reports. |
| C4-03 | pending | Sail | Implement reusable physical dataset/field/type/nullability and executable-expression validation needed by the binding contract. |
| C4-04 | pending | LakeCat | Add only model artifact pointer/hash, physical binding, publication version/CAS, publisher, policy-binding, audit, and outbox state. |
| C4-05 | pending | TypeSec | Add publication/consumption/field/metric/semantic-query/AI-access decisions and signed receipt contracts. |
| C4-06 | pending | Grust | Add reusable semantic model/dataset/field/relationship/metric projection taxonomy and replay tests. |
| C4-07 | pending | LakeCat/QueryGraph | Prove malformed, unauthorized, missing-physical, schema-drift, model-drift, and unknown-version failures close before graph/lineage promotion. |

## Phase 5 — semantic supply chain

| ID | Status | Owner | Unit and acceptance evidence |
| --- | --- | --- | --- |
| C5-01 | pending | QueryGraph | Build reproducible physical TPC-DS Iceberg fixtures through a stock client. |
| C5-02 | pending | QueryGraph/LakeCat | Validate, bind, authorize, CAS-publish, audit, and drain the exact upstream TPC-DS model. |
| C5-03 | pending | Grust/OpenLineage | Project semantic facts and physical/semantic lineage with exact replay evidence. |
| C5-04 | pending | QueryGraph/Sail | Execute representative metrics and bind answers to snapshot, model, policy, plan, graph, and lineage hashes. |
| C5-05 | pending | QueryGraph | Prove deliberate physical, semantic, policy, graph, lineage, and artifact drift invalidates saved proof. |
| C5-06 | pending | QueryGraph | Run the upstream Polaris converter and publish structural, semantic, extension-preservation, and loss reports. |
| C5-07 | pending | all | Deliver and verify a clean-environment one-command operator/client demonstration. |

## Phase 6 — upstream and community release

| ID | Status | Owner | Unit and acceptance evidence |
| --- | --- | --- | --- |
| C6-01 | pending | QueryGraph | Prepare a focused Apache Ossie contribution or public proposal from proven fixtures and loss reports. |
| C6-02 | pending | catalog-bench | Give each catalog maintainer an evidence-linked adapter/result review opportunity and retain corrections. |
| C6-03 | pending | catalog-bench | Publish the quarterly report, immutable sanitized bundle, generated known-gaps page, and reproduction guide. |
| C6-04 | pending | QueryGraph | Produce community presentation/demo material with no claims beyond linked evidence. |
| C6-05 | pending | all | Convert external feedback into a new versioned backlog and record accepted or actively reviewed upstream artifacts. |
