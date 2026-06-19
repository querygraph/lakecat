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

## Pinned Agent Guidance

The current `AGENTS.md` instructions for `/Users/alexy/src/lakecat` are part of
this goal. The 2026-06-19 instruction block supplied in the active thread under
`# AGENTS.md instructions for /Users/alexy/src/lakecat` is a pinned goal input,
not session-local advice. It must continue to govern repo boundaries,
compatibility rules, implementation priorities, Turso usage, verification
gates, commit discipline, and sibling-repo placement until explicitly changed.
When future work changes any of those rules, update both `AGENTS.md` and this
goal in the same logical unit.

The latest pasted `AGENTS.md` block is the current standing operating contract
for LakeCat work. Treat it as goal-level direction even after thread resumes or
context compaction: LakeCat stays thin; reusable Iceberg/engine work goes to
Sail; graph work goes to Grust; governance, TypeDID, secure-agent, and policy
semantics go to TypeSec; QueryGraph remains the end-to-end integration target;
the Rust `turso` crate remains the preferred durable local spine; and every
logical unit updates `CHANGELOG.md`, verifies locally, commits, and pushes only
after the local gates pass.

The canonical agent contract is the repo-local `AGENTS.md` block titled
`LakeCat Agent Guidance`. Treat its repo-boundary, compatibility,
implementation, verification, Turso, book-workflow, and commit/check-in rules
as standing goal requirements. If `AGENTS.md` and this file drift, reconcile
them before choosing the next implementation slice.

The active `AGENTS.md` contract for this goal is:

- LakeCat is the Rust Iceberg-compatible catalog foundation for QueryGraph.
  Keep the catalog boundary thin: identity, tenancy, Iceberg REST
  compatibility, metadata-pointer state, policy gates, and integration events
  belong here.
- Repo boundaries: push reusable Iceberg format, manifest, scan-planning,
  pruning, delete handling, metadata-as-data, and engine work into Sail; push
  graph schema, graph taxonomy, projection logic, graph stores, traversal, and
  graph query behavior into Grust; push governance, policy composition,
  capabilities, TypeDID envelopes, secure agents, and authorization semantics
  into TypeSec; treat QueryGraph as the end-to-end integration target for
  bootstrap, Croissant/CDIF/OSI/ODRL/OpenLineage projection, and QGLake
  acceptance.
- Compatibility rules: do not fork Iceberg semantics, do not require
  non-standard endpoints for normal table access, keep Iceberg metadata
  pristine, prefer typed Sail support for v4 work, use JSON passthrough only as
  a compatibility bridge, and make raw credential vending a deliberate audited
  exception while governed Sail-planned reads remain the default for agents and
  untrusted principals.
- Implementation priorities: use the existing `CatalogStore`,
  `SailCatalogEngine`, `GovernanceEngine`, `CatalogGraphSink`, and
  `LineageSink` seams; keep embedded defaults safe; wire real integrations
  through explicit features; move graph and lineage effects toward a
  transactional outbox; prefer the Rust `turso` crate for the durable local
  catalog spine; do not reintroduce SQLx/SQLite unless explicitly requested.
- Check-in discipline: after every logical unit, update `CHANGELOG.md`, run the
  relevant local gates, stage only the files that belong to the unit, commit,
  and push only after local verification is green.
- Verification preference: use the LakeCat local gates from `AGENTS.md`,
  including focused package tests, `cargo test --workspace --all-features`, and
  `git diff --check`; when touching Sail, Grust, TypeSec, or QueryGraph, run
  focused tests in those sibling repos and report them separately.

LakeCat is the Rust Iceberg-compatible catalog foundation for QueryGraph. Keep
the catalog boundary thin: identity, tenancy, Iceberg REST compatibility,
metadata-pointer state, policy gates, and integration events belong here.

### Pinned Repo Boundaries

- Push Iceberg format, manifest, scan-planning, pruning, delete handling,
  metadata-as-data, and engine work into Sail (`/Users/alexy/src/sail`)
  whenever it can be reusable. LakeCat should prefer Sail APIs and generated
  Iceberg REST models over local reimplementation.
- Push graph schema, graph taxonomy, projection logic, graph stores, traversal,
  and graph query behavior into Grust (`/Users/alexy/src/grust`). LakeCat
  should keep only catalog-facing graph sink/projection boundaries and call
  Grust APIs.
- Push governance, policy composition, capabilities, TypeDID envelopes, secure
  agents, and authorization semantics into TypeSec
  (`/Users/alexy/src/typesec`). LakeCat should ask TypeSec for decisions/proofs
  and persist receipts.
- Treat QueryGraph (`/Users/alexy/src/querygraph`) as the end-to-end
  integration target. LakeCat changes should naturally support QueryGraph
  bootstrap, Croissant/CDIF/OSI/ODRL/OpenLineage projection, and the QGLake
  acceptance flow.

### Pinned Compatibility Rules

- Do not fork Iceberg semantics or make standard clients depend on
  non-standard endpoints for normal table access.
- Keep Iceberg metadata pristine. Business semantics, policy, graph, lineage,
  and agent state should be derived control-plane or graph data, not required
  custom Iceberg metadata.
- For v4 work, prefer typed Sail support when available. JSON passthrough is an
  explicit compatibility bridge, not the long-term implementation.
- Raw credential vending must be a deliberate, audited exception. Governed
  Sail-planned reads are the default path for agents and untrusted principals.

### Pinned Implementation Priorities

- Use the existing trait seams (`CatalogStore`, `SailCatalogEngine`,
  `GovernanceEngine`, `CatalogGraphSink`, `LineageSink`) and keep defaults safe
  for embedded tests.
- Prefer pushing reusable fixes upstream to sibling repos, then depending on
  them from LakeCat. For example, manifest-metric decoding belongs in Sail; a
  reusable catalog graph taxonomy belongs in Grust.
- Keep feature gates honest. Default-feature tests should pass, and real
  integrations should be wired through explicit features such as `sail-local`,
  `typesec-local`, and `grust-local`.
- Side effects to graph and lineage should move toward a transactional outbox
  so catalog state changes are not lost or blocked by external sinks.
- Prefer the Rust `turso` crate for LakeCat's durable local catalog spine. Keep
  the store contract portable, but do not reintroduce SQLx/SQLite unless the
  user explicitly asks for that backend.
- Check in after each logical unit of work. Before committing, add or update
  `CHANGELOG.md` with a concise description of that unit, then stage only the
  files that belong to the unit.

### Pinned Verification

For LakeCat changes, prefer:

- `cargo fmt -p lakecat-sail -p lakecat-service -p lakecat-api -- --check`
- `cargo fmt -p lakecat-cli -- --check` when CLI behavior or fixtures change.
- `cargo test -p lakecat-store --features turso-local`
- `cargo test -p lakecat-service --features turso-local`
- `cargo test -p lakecat-service --all-features`
- `cargo test --workspace --all-features`
- `git diff --check`

When a change touches Sail, Grust, TypeSec, or QueryGraph, run the focused
tests in that sibling repo as well and report each repo separately.

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

## Book Workflow

The LakeCat book is part of the active engineering workflow. Keep it small only
when the implementation is small; as LakeCat gains behavior, grow the book with
substantial, concrete examples that show how the catalog participates in real
workflows from standard clients through agentic systems.

The book is no longer a separate background task. Integrate it into LakeCat's
normal development loop: every meaningful behavior, acceptance flow, or
architecture change should be reflected in the book with examples that are
substantial enough for a user to understand how the catalog is involved in the
workflow.

Book updates should accompany any logical unit that changes public behavior,
operator workflows, QueryGraph handoff behavior, architecture boundaries,
governance semantics, or the way users prove compatibility. Prefer examples
that are runnable or close to runnable, including PySpark/Spark catalog use,
Iceberg REST compatibility, governed Sail-planned reads, credential-vending
decisions, QueryGraph bootstrap/import, Croissant/CDIF/OSI/ODRL/OpenLineage
projection, and agentic QGLake replay.

The book should explain how LakeCat remains a thin catalog while Sail, Grust,
TypeSec, and QueryGraph do their respective jobs. Do not let the book drift
into marketing-only prose; use it as the durable user-facing record of the
workflows that the code, scripts, fixtures, and acceptance checks actually
support.

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
- `cargo fmt -p lakecat-cli -- --check` when CLI behavior or fixtures change.
- `cargo test -p lakecat-store --features turso-local`
- `cargo test -p lakecat-service --features turso-local`
- `cargo test -p lakecat-service --all-features`
- `cargo test --workspace --all-features`
- `git diff --check`

When a change touches Sail, Grust, TypeSec, or QueryGraph, run focused tests in
that sibling repo too and report each repo separately.
