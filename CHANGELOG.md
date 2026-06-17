# Changelog

## Unreleased

- Rejected unsupported Sail `UNIQUE` table constraints in the in-process Iceberg
  provider instead of silently dropping them from generated LakeCat metadata.
- Added nested Iceberg type projection to the in-process Sail provider,
  including struct/list/map parsing into Arrow/DataFusion types and nested field
  id allocation when Sail creates Iceberg metadata.
- Added Iceberg identifier-field projection to the in-process Sail provider:
  Sail primary-key constraints are now written as Iceberg schema
  `identifier-field-ids`, and loaded Iceberg identifier fields are exposed as
  Sail `CatalogTableConstraint::PrimaryKey`.
- Fixed `validate_secret_ref` not re-running on the `upsert_storage_profile`
  path: all three store implementations (no-op default, in-memory, Turso) now
  call `StorageProfile::validate()` before persisting, closing the bypass where
  a profile reconstructed via serde deserialization could be re-stored without
  any validation.
- Fixed `validate_secret_ref` keyword blocklist missing common embedded-secret
  patterns: `api_key=`, `apikey=`, `access_key=`, `private_key=`, `pass=`, and
  `auth=` are now rejected alongside the existing `password=`, `secret=`,
  `token=`, and `credential=` patterns.
- Fixed Iceberg sort-order direction parsing accepting only the 4-char
  abbreviation `"desc"`: `table_sort_fields` now matches both `"desc"` and
  `"descending"` (case-insensitive) so externally-written Iceberg metadata using
  the verbose form is read correctly; fields with an absent or unrecognised
  direction are skipped rather than silently defaulted to ascending.
- Fixed `create_table` in the in-process Sail provider always writing
  `"default-sort-order-id": 1` even for unsorted tables: Iceberg spec §4.1.2
  reserves id 0 for the unsorted order; a non-zero id implies intentional
  sorting and caused clients that issue `assert-default-sort-order-id: 0` on
  subsequent commits to receive a 409 Conflict. Unsorted tables now write id 0;
  the id-0 sentinel entry is included in `sort-orders` in all cases.
- Fixed `TypeSecCredentialIssuer` silently returning `Ok(vec![])` for
  `secret_ref` URIs that use a scheme other than `typesec://` (e.g.
  `vault://`, `aws-sm://`): these schemes pass `validate_secret_ref` at profile
  creation time but were not handled by the TypeSec issuer, returning an empty
  credential list with HTTP 200 rather than surfacing the misconfiguration.  The
  issuer now returns `InvalidArgument` for unsupported schemes.
- Fixed `create_table` handler deriving the stored principal from the
  pre-authorization identity instead of `capability.receipt().principal`:
  the principal embedded in `TableRecord` and the `table.created` audit event
  now consistently comes from the governance receipt, matching all other
  request handlers.
- Fixed `request_identity` computing `content_hash_bytes` twice on the same
  Bearer token bytes: the SHA-256 is now computed once and reused for both the
  principal subject string and the `bearer-token-sha256` envelope field.
- Fixed `x-lakecat-typedid` not selecting `PrincipalKind::Agent`: a caller
  sending only `x-lakecat-typedid` (without `x-lakecat-agent-did`) fell through
  to `Principal::anonymous()` because the principal-selection chain only checked
  `agent_did`. The TypeDID value was captured in the audit envelope but never
  used for authorization, so TypeSec policy ran against the wrong subject.
  `x-lakecat-typedid` is now an independent Agent-principal selector with
  `x-lakecat-agent-did` taking precedence when both headers are present.
- Added Iceberg default sort-order projection to the in-process Sail provider
  so LakeCat `TableStatus.sort_by` reflects Iceberg sort metadata.
- Added a `typesec-local` credential issuer that gates `typesec://` secret-ref
  credential resolution through TypeSec `credentials.issue` policy checks before
  returning scoped short-lived credential config.
- Added a pluggable credential issuer on `LakeCatState`; the default issuer keeps
  remote profiles empty, while integrations can mint scoped short-lived
  credentials from governed storage-profile secret references.
- Added external secret-store references to governed storage profiles, including
  `short-lived-secret-ref` issuance metadata while keeping remote credential
  responses empty until a real issuer is wired in.
- Added sanitized TypeDID/agent request envelopes to authorization receipts so
  governed audit/outbox records carry durable identity context without storing
  raw proof, delegation, bearer-token, or signature material.
- Added basic Iceberg partition-spec projection to the in-process Sail provider
  so Sail `TableStatus` includes partition fields and partition column flags.
- Added Iceberg current-schema column projection to the in-process Sail provider
  so LakeCat tables expose useful Sail `TableStatus` columns.
- Added Sail `CatalogProvider::get_table_commits` support backed by LakeCat's
  memory/Turso metadata pointer log.
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
