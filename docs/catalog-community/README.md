# Catalog Community Program

This directory is LakeCat's architectural entry point for
[`CATALOG-COMMUNITY-PLAN.md`](../../CATALOG-COMMUNITY-PLAN.md). It records what
LakeCat promises at the catalog boundary, how to invoke the neutral lab, and how
evidence moves into the rest of QueryGraph without turning LakeCat into the
benchmark owner, a query engine, a policy engine, a graph database, or a semantic
modeling system.

## Repository ownership

| Concern | Canonical repository | LakeCat responsibility |
| --- | --- | --- |
| Neutral scenarios, catalog adapters, profiles, result schemas, raw evidence, generated matrices | [`querygraph/catalog-bench`](https://github.com/querygraph/catalog-bench) | Keep its own adapter standards-compatible and link exact evidence. |
| Iceberg format interpretation, metadata evolution, physical schema validation, scan planning and execution | [`querygraph/sail`](https://github.com/querygraph/sail) | Call the reusable Sail boundary. |
| Authorization semantics and signed decisions | [`querygraph/typesec`](https://github.com/querygraph/typesec) | Supply typed inputs and persist admitted receipt identities. |
| Semantic graph taxonomy, stores, projection and traversal | [`querygraph/grust`](https://github.com/querygraph/grust) | Emit bounded catalog facts through the sink boundary. |
| Apache Ossie composition, reconciliation, semantic querying and verified answers | [QueryGraph](https://github.com/querygraph) | Store only catalog bindings and immutable artifact/publication state. |
| Catalog CAS, idempotency, audit, outbox, physical identifiers and standard REST behavior | LakeCat | Implement and prove the thin catalog boundary. |

The dependency direction remains `QueryGraph -> LakeCat -> Sail / Grust /
TypeSec`. The neutral harness invokes LakeCat as a peer of every other catalog;
LakeCat does not import the harness or QueryGraph.

## Evidence policy

Two profiles must never be conflated:

- A **historical reproduction profile** freezes the versions, source revisions,
  image digests, build options and validity rules of an already published run.
- A **current candidate profile** freezes the versions selected for the next run.
  It produces no public claim until every referenced image/build artifact has a
  digest and the run emits a complete evidence bundle.

Every scenario result is one of `pass`, `fail`, `unsupported`, or `not-tested`.
Performance eligibility is a derived view, not a fifth correctness outcome. A
failed scenario may retain useful timing samples, but those samples cannot silently
be ranked with passing runs. Raw evidence is immutable; corrections create a new
derived matrix or superseding bundle and retain the original artifacts.

The canonical schema, profile, validation, and matrix commands will be documented
in `catalog-bench` and linked here by exact contract version. Until that Phase 0
unit lands, the existing invocation remains:

```sh
git clone https://github.com/querygraph/catalog-bench.git
cd catalog-bench
cargo run -q -p catalog-bench -- list
```

Production measurements must use the optimized build recipe recorded in the
profile, execute the driver and all catalogs on the same Docker network, and use
the same pinned MinIO instance. A smoke run is not publishable evidence.

## Phase gates

- [Phase 0 baseline](PHASE-0-BASELINE.md) records the exact inventory, upstream
  audit, reproduction findings, version selection, and known claim drift.
- [Backlog](BACKLOG.md) decomposes every phase into independently reviewable and
  verifiable units.
- `DESIGN.md` is authoritative for ownership. It must change before an
  implementation moves responsibilities between repositories.
- A phase closes only when its exit criteria and relevant global acceptance gates
  are recorded with exact commands and outcomes.

No result in this directory is a replacement for the neutral machine-readable
bundle. LakeCat documentation is a consumer of that evidence.
