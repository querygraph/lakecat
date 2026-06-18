# LakeCat Architecture

LakeCat is a Rust-native Iceberg REST catalog and QueryGraph foundation. Its
job is to preserve Iceberg compatibility while moving as much planning,
metadata pruning, commit preparation, and execution-adjacent work as possible
into Sail.

Iceberg format versions 1, 2, and 3 are the compatibility baseline. Format v4
is still under active development, so LakeCat should be v4-ready rather than
claiming settled v4 compatibility: all v4-facing work should enter behind
versioned capability flags, open metadata extension points, and round-trip
compatibility tests.

## Design Goals

- Serve the Iceberg REST Catalog API for existing engines.
- Make Sail the privileged engine path for scans, commits, table maintenance,
  metadata-as-data planning, and future v4 metadata-tree work.
- Keep LakeCat's service layer thin: auth, tenancy, catalog state, API
  compatibility, governance hooks, and event emission.
- Reuse Sail's existing catalog and Iceberg crates instead of forking table
  semantics into LakeCat.
- Use Grust as the semantic graph and relationship index.
- Use TypeSec for RBAC, ODRL policy, typed capabilities, TypeDID envelopes, and
  governed agent access.
- Make QueryGraph the end-to-end integration target from the first milestone.

## Current Local Anchors

Sail already has the right insertion points:

- `sail-catalog` defines the async `CatalogProvider` abstraction, namespaces,
  table/view/database operations, catalog-managed table commits, and commit
  discovery.
- `sail-catalog-iceberg` already carries generated Iceberg REST API models,
  OAuth/bearer token wiring, namespace encoding, table load conversion, and
  catalog commit support.
- `sail-iceberg` already contains DataFusion table providers, manifest pruning,
  metadata-as-data scan paths, Iceberg write/commit plumbing, format-version
  checks, and commit retry/conflict behavior.
- `sail-plan-lakehouse` already binds Sail planning to `sail-iceberg`.

LakeCat now also has a feature-gated in-process `LakeCatCatalogProvider` bridge
that implements Sail's `CatalogProvider` trait over the governed LakeCat store.
That gives Sail a REST-free path for namespace/table resolution and catalog
commits, including pointer-log-backed commit discovery, while keeping reusable
planning and table-status improvements pointed back at Sail. The bridge now
projects basic Iceberg current-schema fields, nested types, partition specs, sort
orders, and identifier fields into Sail `TableStatus`; unsupported `UNIQUE`
constraints are rejected instead of dropped. The conversion should still be
upstreamed as reusable Sail helpers.

QueryGraph already has Rust modules for Sail, Croissant, CDIF, ODRL, DID,
lineage, and lakehouse stories. LakeCat should become the catalog substrate
under that work, not a parallel semantic system.

## High-Level Shape

```text
Spark / Trino / Flink / PyIceberg / QueryGraph clients
  |
  | Iceberg REST Catalog API
  v
LakeCat service
  |-- REST compatibility and OpenAPI conformance
  |-- tenants, projects, warehouses, namespaces, tables, views
  |-- TypeSec checks and credential vending
  |-- durable audit/outbox events
  |-- outbox-drained Grust and OpenLineage projections
  |
  | privileged in-process / gRPC path
  v
Sail catalog and Iceberg runtime
  |-- metadata load and ETag/freshness checks
  |-- remote scan planning
  |-- manifest, partition, delete, stats, and limit pruning
  |-- commit preparation and validation
  |-- metadata-as-data scans
  |
  v
Object storage + metadata store
```

## Workspace Proposal

LakeCat should start as a small workspace:

```text
crates/
  lakecat-core        stable IDs, errors, time, config, content hashes
  lakecat-api         Iceberg REST request/response adapters and OpenAPI tests
  lakecat-store       catalog state repository traits and Turso-backed impls
  lakecat-sail        Sail provider bridge and privileged planning client
  lakecat-graph       catalog-facing Grust sink/adapters
  lakecat-security    TypeSec RBAC/ODRL/TypeDID/credential vending integration
  lakecat-lineage     OpenLineage, audit event hashes, replay/export
  lakecat-querygraph  Croissant/CDIF/OSI/ODRL/OpenLineage bootstrap projections
  lakecat-service     axum server, middleware, auth, metrics
  lakecat-cli         admin, conformance, local demo, QueryGraph bootstrap export
```

The service should use `axum`, `tokio`, `serde`, `tower`, `tracing`,
`object_store`, `turso`, and the Sail crates by path during development. If
LakeCat needs Iceberg structs, prefer reusing or upstreaming them through Sail
instead of creating a second Rust Iceberg model.

## What Belongs In Sail

Push these into `~/src/sail`:

- Shared Iceberg REST models and conversion helpers.
- Catalog-managed commit request/response logic.
- Idempotency-key support for commit/create/drop retries.
- ETag and freshness-aware table loading.
- Remote scan-planning request/response models and table scan lowering.
- Metadata-as-data execution plans for manifests, partition stats, file stats,
  delete indexes, and v4 adaptive metadata structures.
- Rich Iceberg-to-`TableStatus` conversion helpers for in-process catalog
  providers.
- Table-maintenance primitives: expire snapshots, rewrite manifests, compact
  small files, compute statistics, and partition stats scans.
- Extension traits that let an external catalog call Sail for planning without
  depending on LakeCat service internals.

LakeCat should call these Sail capabilities. It should not reimplement manifest
pruning, Iceberg file planning, delete application, or DataFusion physical-plan
construction.

## What Belongs In Grust

Push graph work into `~/src/grust`:

- Catalog graph schema and node/edge taxonomy.
- Reusable graph projection builders for namespaces, tables, columns,
  snapshots, manifests, data/delete files, policies, principals, scan plans,
  commits, and lineage runs.
- Graph stores, traversal indexes, graph query behavior, and SailGraphStore
  lakehouse storage.

LakeCat should call these Grust capabilities through a thin sink boundary. It
should not become the home of reusable graph mechanics.

## What Belongs In LakeCat

Keep these in LakeCat:

- Iceberg REST API server and compatibility surface.
- Tenant/project/warehouse management, inspired by Lakekeeper's separation
  between catalog API and management API.
- Warehouse storage profiles and credential vending.
- External secret-store references on storage profiles, with secret resolution
  and short-lived credential issuance kept outside catalog rows.
- Catalog state persistence and optimistic concurrency.
- Namespace and table lifecycle policy, including soft deletion and restore.
- Governance checks before load, scan-plan, commit, register, drop, and
  credential vending.
- Durable audit/outbox recording, plus thin graph and lineage sink calls when
  draining committed events after successful state transitions.
- QueryGraph semantic projections over catalog objects.

## Entity Model

LakeCat should use the Iceberg-compatible hierarchy externally and a richer
management hierarchy internally:

```text
Server
  Project
    Warehouse
      Namespace*
        Table
        View
```

Namespaces are recursive paths, not filesystem paths. Use LanceDB's namespace
lesson here: keep namespace identity as a vector of validated components, then
let storage profiles resolve physical locations. Never infer authorization from
path strings alone.

Recommended stable IDs:

```text
lakecat:project:{project_id}
lakecat:warehouse:{project_id}:{warehouse}
lakecat:namespace:{warehouse}:{path_hash}
lakecat:table:{warehouse}:{namespace}:{name}
lakecat:snapshot:{table_id}:{snapshot_id}
lakecat:scan-plan:{sha256}
lakecat:commit:{table_id}:{sequence}:{sha256}
```

## Compatibility API

The Iceberg REST API should be served at:

```text
/catalog/v1/{prefix}/...
```

LakeCat management APIs should be separate:

```text
/management/v1/projects
/management/v1/warehouses
/management/v1/policies
/management/v1/lineage
/management/v1/graph
```

QueryGraph bootstrap and semantic-publication APIs should be explicit:

```text
/querygraph/v1/bootstrap
```

This keeps engines talking to a standard catalog while QueryGraph and operators
get richer controls.

## Sail-First Remote Planning

Remote scan planning should be a first-class LakeCat feature because it is the
place where a catalog can stop being a passive metadata pointer service.

Flow:

1. Client calls Iceberg REST scan-planning endpoint.
2. LakeCat authenticates the principal and loads table state.
3. TypeSec checks whether the principal can plan this table, columns,
   partitions, row filters, and credential scope.
4. LakeCat asks Sail to plan the scan against the current metadata pointer,
   requested projection, filters, limit, point-in-time snapshot, or incremental
   start/end snapshot range.
5. Sail performs manifest, partition, stats, delete, and limit pruning using
   its Iceberg/DataFusion code. The current local implementation already uses
   Sail manifest-list I/O for append-only incremental parent-chain planning,
   including delete-file reference matching for added delete manifests, and
   rejects overwrite/delete incremental operations until their semantics are
   wired end to end.
   REST filter expressions are validated against Sail's generated Iceberg REST
   models and schema metadata, preserved in structured opaque plan-task tokens,
   and applied conservatively to file bounds during local manifest expansion
   whenever metrics are present.
6. LakeCat returns Iceberg file scan tasks and records a scan-plan event.
7. QueryGraph receives graph and lineage edges for who planned what, from which
   snapshot, under which policy.

Sail remains the optimizer. LakeCat remains the policy-aware catalog facade.

## Commit Path

Commit handling should be optimistic, idempotent, and auditable:

1. Validate request shape and idempotency key.
2. Authenticate principal and check TypeSec capabilities.
3. Load current table metadata pointer from `lakecat-store`.
4. Delegate Iceberg update validation and action assembly to Sail.
5. Persist new metadata file through the storage profile.
6. Atomically compare-and-swap the table metadata pointer in `lakecat-store`.
7. Store the idempotency record and response.
8. Record audit/outbox events with the committed transaction, then drain graph,
   lineage, and QueryGraph semantic projections from that durable outbox.

The compare-and-swap record should include:

```text
table_id
previous_metadata_location
new_metadata_location
snapshot_id
sequence_number
format_version
principal
policy_hash
idempotency_key
request_hash
response_hash
committed_at
```

## Store Backends

Start with Turso for local durable demos/tests and keep the storage contract
portable enough for remote Turso/libSQL or a later production backend.

Current implementation status: `lakecat-store` has an opt-in `turso-local`
feature with a Turso-backed `TursoCatalogStore` for namespaces, tables, metadata
pointer history, idempotency records, audit events, and outbox rows. The service
binary uses it when built with `turso-local` and `LAKECAT_TURSO_PATH` is set.
Table commits now write metadata objects through the Rust `object_store` URL
dispatch seam when commit plans carry new metadata, advance table pointers
through compare-and-swap, persist idempotency/audit/outbox records, and expose a
service-level drain that projects committed events to graph and lineage sinks.
The local `file://` path remains the verified default, while configured remote
stores can plug into the same writer boundary. A typed storage-profile model now
drives conservative credential responses: embedded `file://` tables can return
scoped no-secret profile hints, while remote object stores can reference external
secret stores and still return no credentials until short-lived issuance is
implemented. Credential vending now goes through a `CredentialIssuer` integration
hook on the service state: the default issuer is conservative, while the
`typesec-local` issuer gates `typesec://` secret-ref resolution through TypeSec
`credentials.issue` checks before returning scoped short-lived credential config.
For local and CI-style deployments, `typesec://env/VARIABLE` resolves credential
config from process environment after authorization; raw values still never live
in catalog rows. Cloud-specific resolver backends can plug into that boundary
without storing raw secrets in catalog state. Production secret-ref URI schemes
(`vault://`, `aws-sm://`, `gcp-sm://`, and `azure-kv://`) now dispatch through
the same TypeSec authorization boundary. `vault://` refs can resolve through a
Vault HTTP backend when `LAKECAT_VAULT_ADDR` / `LAKECAT_VAULT_TOKEN` (or the
standard `VAULT_ADDR` / `VAULT_TOKEN`) are configured; the remaining production
providers fail closed with explicit not-configured errors until their SDK
resolvers are enabled. Governed management endpoints can now upsert and list
warehouse storage profiles, and Turso persists those profiles for
longest-prefix credential selection. Production external secret-store resolver
backend coverage beyond Vault remains pending. Governed management endpoints can
also upsert and list durable semantic view records in memory and Turso, giving
QueryGraph a stable catalog-owned view entity before standard Iceberg view REST
semantics are wired. QueryGraph bootstrap now exports those stored views with
manifest-covered OSI handoff hashes, view-aware graph edges, and OpenLineage view
counts. Governed management endpoints now also upsert and list durable server
records in memory and Turso, establishing the Server > Project > Warehouse
control-plane hierarchy without changing standard table access. Governed table
lifecycle now records soft-delete rows, hides deleted tables from normal catalog
reads, restores soft-deleted tables through a governed management endpoint, and
emits `table.deleted` / `table.restored` audit/outbox events. Governed policy
management endpoints can now upsert/list enforced ODRL policy bindings, and
active table bindings are attached to authorization context before TypeSec runs.

Required tables:

- `projects`
- `warehouses`
- `storage_profiles`
- `namespaces`
- `tables`
- `views`
- `metadata_pointer_log`
- `idempotency_records`
- `soft_deletes`
- `policy_bindings`
- `audit_events`

Keep object storage as the source of Iceberg metadata files and the relational
store as the atomic pointer and management state. This mirrors Iceberg's catalog
contract and avoids turning LakeCat into a proprietary table format.

## Grust Graph Model

LakeCat graph updates should be written through Grust, with Sail-backed graph
storage as the preferred production path when available.

Node labels:

- `Project`
- `Warehouse`
- `Namespace`
- `Table`
- `View`
- `Column`
- `Snapshot`
- `Manifest`
- `DataFile`
- `DeleteFile`
- `PartitionSpec`
- `SortOrder`
- `Policy`
- `Principal`
- `CredentialScope`
- `ScanPlan`
- `Commit`
- `LineageRun`
- `QueryGraphModel`

Edge labels:

- `CONTAINS`
- `DESCRIBES`
- `CURRENT_SNAPSHOT`
- `HAS_COLUMN`
- `HAS_MANIFEST`
- `HAS_DATA_FILE`
- `HAS_DELETE_FILE`
- `GOVERNED_BY`
- `CAN_READ`
- `CAN_PLAN`
- `CAN_COMMIT`
- `USED_BY`
- `DERIVED_FROM`
- `EMITTED`
- `ATTESTED_BY`

Grust should own graph schema, typed/untyped graph operations, indexing, and
traversals. LakeCat should only translate committed catalog events into bounded
semantic graph mutations, send them through the durable outbox, and expose graph
reads needed by QueryGraph. High-cardinality file and manifest facts should stay
queryable as Iceberg/Sail metadata-as-data unless Grust provides a reusable
taxonomy and storage strategy for them.

## TypeSec Governance

Every externally meaningful operation should pass through TypeSec:

- `catalog.config`
- `namespace.create`
- `namespace.drop`
- `table.create`
- `table.load`
- `table.plan_scan`
- `table.commit`
- `table.register`
- `table.drop`
- `credentials.vend`
- `graph.read`
- `lineage.read`

Policy decisions should consider:

- principal DID / TypeDID envelope;
- warehouse and namespace scope;
- table, column, partition, and snapshot scope;
- intended action;
- requested credential duration;
- ODRL usage constraints;
- whether the caller is a human, service, or agent.

LakeCat captures a sanitized `lakecat.request-identity.v1` envelope from
principal, bearer-token, TypeDID, delegation, and agent-summary headers before it
calls TypeSec. Proof-like material is reduced to SHA-256 hashes and the envelope
is marked `unverified` until TypeSec/TypeDID verification is plugged in. The
authorization receipt, including this request context, should be persisted with
the audit event and attached to QueryGraph lineage.

## QueryGraph Integration

LakeCat should publish a QueryGraph bundle for every warehouse:

```text
Croissant/CDIF projection
  tables, columns, files, examples, licenses, access metadata

OSI handoff
  stable dataset and field anchors for QueryGraph-owned semantics

Grust graph
  physical + semantic + policy + lineage relationships

TypeSec policies
  RBAC, ODRL, capabilities, TypeDID trust anchors

OpenLineage
  catalog changes, scan plans, commits, table maintenance, agent answers
```

QueryGraph's Rust service should be able to bootstrap from LakeCat with:

```text
querygraph import-lakecat --catalog http://localhost:8181/catalog \
  --warehouse local --build-bundle --load-graph --verify-policy
```

Current LakeCat bootstrap bundles include a `manifest` that hashes each emitted
Croissant, CDIF, OSI handoff, ODRL, catalog graph, and OpenLineage artifact.
QueryGraph importers should verify those hashes before loading the semantic
bundle or asking Grust to ingest catalog graph projections. LakeCat may publish
stable dataset and field anchors for OSI import, but QueryGraph owns metrics,
dimensions, joins, business ontology, and authoritative semantic names.
The corresponding `querygraph.bootstrap` audit/outbox event carries the verified
bundle, graph, and OpenLineage hashes so downstream lineage replay can prove it
is describing the same bootstrap material QueryGraph imported.

## Lakekeeper Lessons To Adopt

- Separate standards-compatible catalog API from management API.
- Treat warehouse storage profiles and credentials as first-class objects.
- Do not share warehouse locations across tenants/projects.
- Prefer external identity providers and avoid storing user secrets.
- Support optional event sinks.
- Support soft deletion and restore.
- Keep data-contract hooks in the catalog lifecycle.

## LanceDB Lessons To Adopt

- Model namespaces as recursive logical paths.
- Keep namespace client APIs independent from backend implementation.
- Make catalog organization useful for AI workloads, not only SQL engines.
- Preserve embedded/local mode as a first-class developer experience.

## Sail Lessons To Adopt

- Use Spark compatibility as a client surface, not an implementation constraint.
- Keep compute Rust-native with Arrow/DataFusion execution.
- Make metadata planning part of execution, not a separate service tax.
- Keep parser, analyzer, spec, and planner paths as the canonical extension
  route for new semantics.

## Initial Milestones

1. Scaffold the Rust workspace with `lakecat-core`, `lakecat-store`,
   `lakecat-service`, and `lakecat-api`.
2. Add a minimal Iceberg REST config, namespace, create/load table, and commit
   endpoint backed by memory or opt-in Turso.
3. Move or expose shared REST models and idempotency helpers in Sail.
4. Wire LakeCat to Sail's Iceberg commit validation and table-status conversion.
5. Add TypeSec policy checks for load, commit, scan planning, graph reads, and
   credential-vending requests.
6. Add durable outbox delivery for namespace/table/scan/commit graph and
   lineage projections.
7. Add remote scan planning through Sail and return Iceberg scan tasks.
8. Add QueryGraph bootstrap/export with Croissant, CDIF, policies, and lineage.
9. Add production external secret-store resolver backends and OIDC-aware
   credential issuance.
10. Push reusable catalog graph taxonomy into Grust and consume it from LakeCat.
11. Add v4-ready metadata extension tests as the Iceberg v4 spec settles.

## Non-Goals

- Do not invent a LakeCat table format.
- Do not bypass Iceberg metadata semantics for speed.
- Do not embed business semantics directly into Iceberg metadata as the only
  source of truth.
- Do not make LakeCat own graph algorithms; use Grust.
- Do not make LakeCat own agent security; use TypeSec.
- Do not make QueryGraph depend on non-standard catalog endpoints for normal
  Iceberg table access.
