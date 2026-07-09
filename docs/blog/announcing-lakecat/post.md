# LakeCat 0.3 "Ocelot": governed Iceberg REST with proof built in

LakeCat is a Rust-native, Iceberg-compatible **REST catalog foundation** for QueryGraph. It keeps stock Iceberg clients on ordinary REST catalog paths while binding catalog state, Sail planning, TypeSec receipts, Grust projection, OpenLineage evidence, and QueryGraph handoff to the same accepted table transition.

Ocelot is the release where that posture becomes announcement-ready: LakeCat's full local release-candidate gate passed from a clean tree, including the QGLake handoff proof:

```text
QGLake handoff verified
LakeCat release-candidate checks passed from a clean tree.
```

That matters because LakeCat is not trying to win by being another catalog-shaped service with a hopeful integration story. It is trying to make the catalog boundary verifiable.

## A thin catalog with a durable spine

LakeCat keeps the catalog boundary deliberately thin. Identity and tenancy, Iceberg REST compatibility, metadata-pointer state, policy gates, and integration events live in LakeCat. Reusable engines stay in sibling projects:

- **Sail** owns Iceberg format, scan planning, pruning, and engine behavior.
- **Grust** owns graph projection, traversal, Cypher/GQL behavior, and durable graph backends.
- **TypeSec** owns policy, typed authority, receipts, and agent/security semantics.
- **QueryGraph** owns the higher-level semantic import, verification, and navigator story.

Underneath LakeCat is a durable Turso spine. Every accepted commit can bind together the table transition, compare-and-swap pointer movement, audit evidence, transactional outbox rows, and idempotency. A retry replays the prior result instead of double-applying. A malformed replay is rejected before graph or lineage projection.

## Governance and lineage are not side effects

Governed reads narrow projection and apply required filters. Credential vending is policy-bound and audited. Table, view, scan, credential, management, and policy events drain from the outbox only after the catalog transaction commits.

The lineage side is OpenLineage-shaped, but the more important word is **verifiable**. The local proof checks that the drained lineage artifact, the LakeCat replay output, the QueryGraph import plan, and the graph projection evidence agree with the same source transition. If they do not agree, the handoff is rejected.

That makes LakeCat interesting to OpenLineage users: not just "we emitted an event," but "this lineage evidence can be replayed and checked against the catalog's own audit trail."

## The QueryGraph handoff

The Ocelot gate runs `scripts/qglake-handoff-local.sh`. It starts LakeCat locally, creates a fixture, plans through Sail, writes Turso catalog state, projects catalog events into a Grust Turso graph, drains OpenLineage evidence, and then runs QueryGraph's locked verify/import commands over the same bundle.

The handoff produces four core artifacts:

- `lakecat-bootstrap.json`
- `lineage-drain.json`
- `querygraph-import-plan.json`
- `handoff-summary.json`

The summary is schema-closed. It carries verified table/view counts, semantic hashes, OpenLineage and replay evidence, graph projection proof, captured command output hashes, and the QueryGraph verify/import results. Paths must resolve under the handoff directory before LakeCat hashes or parses them. Extra proof claims are rejected rather than ignored.

This is the piece that makes LakeCat a QueryGraph foundation instead of only an Iceberg catalog.

## Why Iceberg, Polaris, and OpenLineage people should care

LakeCat speaks the standard Iceberg REST shape and deliberately does not try to fork the table format. The long-term fit is complementary:

- **Iceberg** gets a catalog implementation that treats table state, scan restrictions, credential proof, and lineage as first-class evidence.
- **Polaris** remains a natural catalog-layer neighbor: QueryGraph/LakeCat can act as a semantic export and proof adjunct rather than a replacement.
- **OpenLineage** gets a concrete governed-catalog testbed where lineage artifacts are replayable and hash-bound to catalog state.
- **Spark/Sail** get a place to attach governed scan context without turning every engine into a policy system.

Ocelot also moves LakeCat onto the current QueryGraph substrate wave: **Grust 0.12 "Lobster"** and **TypeSec 0.12 "Torcello"**. Grust provides the graph/Cypher substrate; TypeSec provides the typed authorization and receipt fabric.

## What is in scope

Ocelot's release scope is intentionally narrower than the whole QueryGraph vision:

- stock-client Iceberg REST catalog behavior,
- Turso-backed catalog state,
- compare-and-swap commit discipline,
- idempotency and audit/outbox replay,
- governed Sail-planned access,
- redacted credentials and TypeSec policy boundaries,
- OpenLineage and Grust projection boundaries,
- QGLake handoff proof into QueryGraph.

Typed Iceberg v4 semantics, richer reusable graph algorithms, cloud SDK secret managers, and full QueryGraph product semantics remain in Sail, Grust, TypeSec, and QueryGraph respectively. That boundary is the point. LakeCat should stay thin enough to be trusted.

## Try it

```sh
git clone https://github.com/querygraph/lakecat
cd lakecat
cargo build
scripts/check-release-readiness.sh --quick
```

For the full local proof:

```sh
CARGO_BUILD_JOBS=1 scripts/check-release-readiness.sh --release-candidate
```

Read next:

- `README.md` for the local run and handoff path.
- `RELEASE.md` for the release-candidate proof contract.
- `DESIGN.md` for ownership boundaries.
- `docs/book/lakecat.md` for the full narrative.

LakeCat is not the whole platform. It is the catalog boundary that makes the platform's claims checkable.
