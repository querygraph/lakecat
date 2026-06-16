# Changelog

## Unreleased

- Added a feature-gated in-process Sail `CatalogProvider` bridge that lets Sail
  resolve governed LakeCat namespaces and tables without a REST hop.
- Added governed table restore, including a management restore endpoint,
  table-scoped restore capability, memory/Turso soft-delete removal, and
  durable `table.restored` audit/outbox records with OpenLineage projection.
- Reconciled the architecture and OPUS1 working-plan docs with the governed
  table soft-delete implementation.
- Added governed table soft deletion, including catalog `DELETE` handling,
  memory/Turso soft-delete records, hidden deleted tables in normal reads, and
  durable audit/outbox projection for `table.deleted`.
- Reconciled the architecture and OPUS1 working-plan docs with the governed
  ODRL policy-binding management implementation.
- Added governed policy-binding management for ODRL documents, with memory/Turso
  persistence and active table bindings attached to authorization context.
- Reconciled the architecture and OPUS1 working-plan docs with the governed
  storage-profile management implementation.
- Added governed management endpoints and durable store support for warehouse
  storage profiles, including longest-prefix profile selection for credential
  responses.
- Updated the architecture and OPUS1 working-plan docs to mark storage-profile
  modeling as started while keeping remote credential issuance and management
  APIs as pending work.
- Added a typed storage-profile model for credential vending, returning scoped
  no-secret `file://` profile hints while keeping remote object-store credentials
  empty until short-lived issuance is implemented.
- Reconciled the OPUS1 working-plan and architecture docs with the current
  implementation status for Turso CAS commits, local object metadata writes,
  durable outbox draining, OpenLineage projection, and remaining storage-profile,
  Sail Tier-1, TypeDID, and Grust-taxonomy work.
- Added catalog-level OpenLineage projection in `lakecat-lineage`, including
  OpenLineage event hashing in the default lineage sink for outbox-drained
  table and namespace operations.
- Added HMAC-signed Sail plan-task envelopes for new scan-planning tokens while
  keeping legacy unsigned structured tokens decodable for compatibility.
- Added a capability-gated Iceberg REST table credentials endpoint that audits
  credential-vending attempts and returns no raw storage secrets until storage
  profiles can issue short-lived credentials safely.
- Added a GitHub Actions Rust CI matrix for default workspace tests plus
  `sail-local`, `typesec-local`, `grust-local`, `turso-local`, and all-features
  rows, with sibling Sail/Grust/TypeSec checkouts matching LakeCat path deps.
- Removed inline graph and lineage side effects from catalog request handlers;
  durable outbox events are now the delivery path for table/namespace
  graph-lineage projections.
- Added a service-level outbox drain that projects durable
  `lakecat.lineage-and-graph` events into the graph and lineage sinks and marks
  them delivered after successful sink projection.
- Added typed catalog-config and namespace capabilities plus durable
  `catalog.config-read`, `namespace.created`, and `namespace.listed`
  audit/outbox events for the remaining catalog-scope read/write paths.
- Added a typed graph-read capability and durable `querygraph.bootstrap`
  audit/outbox event so QueryGraph bootstrap reads carry a replayable governance
  proof without moving graph behavior into LakeCat.
- Added a typed table-create capability and durable `table.created` audit/outbox
  events so table creation is governed and replayable through the catalog outbox.
- Added a typed table-commit capability so metadata commits must carry a minted
  governance proof before Sail prepares metadata and Turso advances the pointer.
- Added a typed table-load capability and durable `table.loaded` audit/outbox
  events so metadata reads leave the same governance trail as governed scans.
- Added durable audit/outbox recording for governed scan-task fetches, carrying
  the table-scan capability receipt, plan-task token, and materialized task
  counts.
- Added durable Turso audit/outbox recording for governed scan planning events,
  including the typed table-scan authorization receipt and Sail plan summary.
- Required the typed table-scan capability for fetch-scan-tasks as well as scan
  planning, keeping task materialization on the governed Sail read path.
- Added a typed table-scan capability and routed scan planning through a helper
  that requires the minted governance proof before invoking Sail.
- Persisted commit authorization receipts into Turso audit and outbox payloads,
  keeping TypeSec governance decisions attached to the durable commit record.
- Added a Turso concurrent commit regression that exercises two writers racing on
  the same metadata pointer and verifies compare-and-swap admits only one.
- Added a typed catalog outbox drain API and Turso implementation so commit
  events can be fetched by sink and marked delivered without coupling graph or
  lineage side effects to the request path.
- Added local `file://` object-store metadata writes for commit plans that carry
  new metadata, keeping the Sail-prepared metadata JSON, the written metadata
  object, and the Turso CAS table record in sync.
- Added a LakeCat `metadata-location` commit extension that the Sail-facing
  commit plan validates alongside standard Iceberg REST requirements and threads
  through Turso pointer CAS; aligned the local Grust path dependency with the
  current Grust 0.9 workspace.
- Added metadata pointer compare-and-swap enforcement to Turso commits, including
  expected previous pointer tracking, pointer movement, idempotent replay, and a
  stale-pointer conflict regression test.
- Scaffolded LakeCat as a Rust workspace for an Iceberg-compatible catalog and
  QueryGraph foundation.
- Added REST catalog handlers, typed principal resolution, and integration seams
  for Sail, TypeSec, Grust, OpenLineage, OSI, Croissant, ODRL, and QueryGraph
  bootstrap projection.
- Added Sail-backed scan planning for local Iceberg metadata, including
  structured table-bound plan-task tokens, incremental append planning, delete
  file references, and conservative bounds pruning.
- Added a Rust-native Turso local durable catalog store behind `turso-local`,
  including namespaces, table records, metadata pointer log, idempotency replay,
  audit events, and outbox events.
- Added repo guidance in `AGENTS.md`: push graph behavior into Grust, Iceberg
  and planning work into Sail, governance into TypeSec, and describe each
  logical unit in this changelog before committing.
