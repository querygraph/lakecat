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

## Agent Guidance

Treat the guidance in `AGENTS.md` as part of this goal, not as separate
session-only advice. Keep this file and `AGENTS.md` aligned when the operating
model changes.

- LakeCat is the Rust Iceberg-compatible catalog foundation for QueryGraph.
  Keep its boundary thin: identity, tenancy, Iceberg REST compatibility,
  metadata-pointer state, policy gates, and integration events belong here.
- Push reusable engine, graph, and governance work into Sail, Grust, and
  TypeSec respectively, then integrate those capabilities through LakeCat's
  catalog-facing seams.
- Preserve Iceberg compatibility first. Standard clients should keep using
  standard catalog behavior; graph, lineage, policy, semantic, and agent state
  must remain derived control-plane evidence rather than custom Iceberg
  metadata required for normal access.
- Use QueryGraph as the end-to-end target. LakeCat should make OSI,
  OpenLineage, Croissant, ODRL, TypeSec, and QGLake evidence replayable without
  turning the catalog core into the full QueryGraph semantic layer.
- Keep durable local catalog work on the Rust `turso` crate unless explicitly
  directed otherwise, while preserving the portability of the `CatalogStore`
  contract.
- After each logical unit, update `CHANGELOG.md`, commit only the related
  files, and keep `STATUS.md` current when the unit changes the working state
  or next-step guidance.

Permanent operating constraints from `AGENTS.md`:

- LakeCat should keep only the catalog-facing boundary for graph, governance,
  lineage, and engine integrations. If the work becomes reusable graph schema,
  taxonomy, projection, traversal, graph storage, or graph query behavior, move
  it into Grust first and integrate through a LakeCat sink or projection seam.
- If the work becomes reusable Iceberg format handling, manifest processing,
  scan planning, pruning, delete handling, metadata-as-data, or execution
  behavior, move it into Sail first and call it from LakeCat.
- If the work becomes reusable governance, policy composition, capabilities,
  TypeDID envelopes, secure-agent behavior, or authorization semantics, move it
  into TypeSec first and persist the resulting decisions, proofs, and receipts
  in LakeCat.
- Keep OSI, OpenLineage, Croissant, ODRL, and TypeSec integration replayable as
  catalog evidence in LakeCat while leaving richer semantic composition and
  agent workflows to QueryGraph or QueryGraph plugins/add-ons.
- Use the Rust `turso` crate for the durable local catalog spine unless the
  user explicitly requests another backend.
- Keep feature gates honest: default-feature tests should remain lightweight
  and embedded, while real integrations must stay behind explicit features such
  as `sail-local`, `typesec-local`, `grust-local`, and `turso-local`.
- Before committing, update `CHANGELOG.md`; after each logical unit, stage only
  the related files, commit them, update `STATUS.md` when the working state or
  next slice changes, verify locally, and push only after the local proof is
  green.

## Repo Boundaries

- Push reusable Iceberg format, manifest, scan-planning, pruning, delete
  handling, metadata-as-data, and engine work into Sail
  (`/Users/alexy/src/sail`) whenever it can be reusable. LakeCat should prefer
  Sail APIs and generated Iceberg REST models over local reimplementation.
- Push graph schema, taxonomy, projection, traversal, query, storage, and Cypher
  behavior into Grust (`/Users/alexy/src/grust`). LakeCat may keep only the
  catalog-facing sink/projection boundary and must call Grust APIs.
- Push governance, policy composition, TypeDID envelopes, secure agents,
  capabilities, and authorization semantics into TypeSec
  (`/Users/alexy/src/typesec`). LakeCat asks TypeSec for decisions and proofs,
  then persists receipts.
- Treat QueryGraph (`/Users/alexy/src/querygraph`) as the end-to-end acceptance
  target. LakeCat should naturally support QueryGraph bootstrap, QGLake flows,
  Croissant/CDIF/OSI/ODRL/OpenLineage projection, and governed agent access
  without importing QueryGraph.
- Keep OSI, OpenLineage, Croissant, ODRL, and TypeSec integration as a
  catalog-facing contract in LakeCat and a richer QueryGraph integration layer
  outside the catalog core. LakeCat should emit, persist, and verify the
  receipts and projections that make those standards replayable; QueryGraph can
  own higher-level semantic composition, agent workflows, and plugin/add-on
  behavior over those contracts.
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
- Keep feature gates honest. Default-feature tests should pass, and real
  integrations must remain explicit rather than quietly becoming required for
  embedded or unit-test use.
- Keep LakeCat's durable local spine on the Rust `turso` crate. The
  `CatalogStore` contract should stay portable, but SQLx/SQLite should not be
  reintroduced unless explicitly requested.

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

Do not rely on cloud CI as the first proof that a unit works. CI should remain
manual-only until the local gates are known to pass, and failed cloud runs must
not substitute for local diagnosis.

When implementation touches graph, governance, or engine semantics, first ask
whether the reusable work belongs in Grust, TypeSec, or Sail. LakeCat should
keep the catalog-facing seam, receipts, and compatibility behavior; the sibling
repo should receive the reusable capability.

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
