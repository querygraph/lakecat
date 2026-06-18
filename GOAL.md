# LakeCat Goal

Keep building LakeCat as the Rust-native, Iceberg-compatible catalog foundation
for QueryGraph, following the current design documents and the live repository
state rather than stale chat context.

## Objective

LakeCat should become a new-generation catalog that preserves Iceberg client
compatibility while bringing the Rust execution, planning, governance, graph,
and lineage stack as close to the data as possible.

The catalog boundary stays thin and durable:

- Serve standard Iceberg REST catalog behavior for normal table access.
- Own identity, tenancy, metadata-pointer state, compare-and-swap commits,
  idempotency, audit, outbox, policy gates, and integration events.
- Keep Iceberg metadata pristine. Business semantics, graph, lineage,
  governance, and agent state are derived control-plane or graph data.
- Prefer governed Sail-planned reads for agents and untrusted principals; raw
  credential vending is an explicit audited exception.

## Source Of Truth

Before choosing or implementing a slice, read the current state from:

- `AGENTS.md` for repo boundaries, feature gates, commit discipline, and
  verification expectations.
- `STATUS.md` for the latest committed/pushed state, known blockers, and next
  recommended slice.
- `ARCHITECTURE.md` for the target architecture and placement rules.
- `docs/OPUS1.md` for the review findings and milestone intent.
- `docs/OPUS1-DESIGN.md` for the living review log and working plan.
- `docs/OPUS2-DESIGN.md` for the current QueryGraph, outbox, lineage, book,
  and acceptance-test work plan.
- The actual code, manifests, tests, and sibling repo APIs in the current
  checkout.

If these conflict, prefer the live code and the newest status/design entry, then
update the docs as part of the logical unit.

## Repo Boundaries

- Push reusable Iceberg format, manifest, scan-planning, pruning, delete
  handling, metadata-as-data, and engine work into Sail.
- Push graph schema, taxonomy, projection, traversal, query, storage, and Cypher
  behavior into Grust. LakeCat may keep only the catalog-facing sink/projection
  boundary and must call Grust APIs.
- Push governance, policy composition, TypeDID envelopes, secure agents,
  capabilities, and authorization semantics into TypeSec. LakeCat asks TypeSec
  for decisions and proofs, then persists receipts.
- Treat QueryGraph as the end-to-end acceptance target. LakeCat should naturally
  support QueryGraph bootstrap, QGLake flows, Croissant/CDIF/OSI/ODRL/OpenLineage
  projection, and governed agent access without importing QueryGraph.
- Treat the LakeCat book as part of the development workflow, not a side
  artifact. Keep growing it as implementation lands, with substantial
  end-to-end examples that show how the catalog participates in real user
  workflows: standard Iceberg clients, PySpark/Spark, governed scan planning,
  credential vending decisions, QueryGraph bootstrap/import, OpenLineage replay,
  and agentic QGLake flows.

## Build Direction

Continue moving toward:

- A durable Turso-backed local catalog spine with portable `CatalogStore`
  semantics. Prefer the Rust `turso` crate for the durable embedded/local spine
  and do not reintroduce SQLx/SQLite unless explicitly requested.
- In-process Sail catalog/provider integration so policy and planning fuse
  without unnecessary REST indirection.
- Standard Iceberg REST compatibility plus typed v4-ready extension handling.
- Transactional outbox-driven graph and lineage side effects.
- TypeSec-backed unbypassable authorization for every privileged path.
- Grust-backed catalog graph projections that can be consumed through Grust
  query surfaces such as Cypher without making LakeCat a graph engine.
- QueryGraph acceptance flows that prove LakeCat is the catalog substrate, not a
  standalone demo.

## Compatibility Rules

- Do not fork Iceberg semantics or make standard clients depend on non-standard
  endpoints for normal table access.
- Keep Iceberg metadata pristine. Business semantics, policy, graph, lineage,
  and agent state are derived control-plane or graph data, not required custom
  Iceberg metadata.
- For v4 work, prefer typed Sail support when available. JSON passthrough is a
  compatibility bridge, not the long-term implementation.
- Raw credential vending must be deliberate and audited. Governed Sail-planned
  reads are the default path for agents and untrusted principals.

## Implementation Priorities

- Use the existing trait seams: `CatalogStore`, `SailCatalogEngine`,
  `GovernanceEngine`, `CatalogGraphSink`, and `LineageSink`.
- Keep defaults safe for embedded tests while wiring real integrations behind
  explicit features such as `sail-local`, `typesec-local`, `grust-local`, and
  `turso-local`.
- Prefer pushing reusable fixes upstream to sibling repos, then depending on
  them from LakeCat. Manifest-metric decoding belongs in Sail; reusable catalog
  graph taxonomy belongs in Grust; reusable governance and agent authorization
  semantics belong in TypeSec.
- Move graph and lineage side effects toward the transactional outbox so catalog
  state changes are not lost or blocked by external sinks.

## Working Rule

For each logical unit:

1. Inspect current docs, manifests, code, and sibling repo APIs first.
2. Implement the smallest slice that makes the requested LakeCat end state more
   true without moving reusable Sail, Grust, or TypeSec responsibilities into
   LakeCat.
3. Update `CHANGELOG.md` and any affected design/status docs before committing.
4. Update the book when the unit changes user-facing workflows or architecture,
   especially with runnable or near-runnable examples instead of prose-only
   claims.
5. Run focused tests plus the relevant LakeCat gates from `AGENTS.md`.
6. Commit only the files belonging to that logical unit, then push when local
   verification is green.

## Verification Preference

For LakeCat changes, prefer these local gates before pushing:

- `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`
- `cargo test -p lakecat-store --features turso-local`
- `cargo test -p lakecat-service --features turso-local`
- `cargo test -p lakecat-service --all-features`
- `cargo test --workspace --all-features`
- `git diff --check`

When a change touches Sail, Grust, TypeSec, or QueryGraph, run focused tests in
that sibling repo too and report each repo separately.
