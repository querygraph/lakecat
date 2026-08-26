# Iceberg REST Table Lifecycle

LakeCat implements the ordinary Apache Iceberg REST table boundary without
requiring a LakeCat-specific client. This document is the durable operator and
contributor contract for create, list, load, update, drop, registration, and
rename behavior accepted by the catalog-neutral C1-05 scenario.

The neutral scenario, comparison matrix, transcript hashes, and exact production
artifacts live in
[`catalog-bench`](https://github.com/querygraph/catalog-bench/blob/6bc668b/docs/TABLE-CONFORMANCE.md).
LakeCat owns only its catalog behavior, governance boundary, persistence, and
evidence projection.

## Standard routes

Every route has a canonical default-warehouse form and, where Iceberg defines a
prefix, a warehouse-prefixed form:

| Operation | Method and route |
| --- | --- |
| Create table | `POST /catalog/v1[/<warehouse>]/namespaces/<namespace>/tables` |
| List tables | `GET /catalog/v1[/<warehouse>]/namespaces/<namespace>/tables` |
| Load table | `GET /catalog/v1[/<warehouse>]/namespaces/<namespace>/tables/<table>` |
| Update table | `POST /catalog/v1[/<warehouse>]/namespaces/<namespace>/tables/<table>` |
| Drop table | `DELETE /catalog/v1[/<warehouse>]/namespaces/<namespace>/tables/<table>` |
| Register metadata | `POST /catalog/v1[/<warehouse>]/namespaces/<namespace>/register` |
| Rename table | `POST /catalog/v1[/<warehouse>]/tables/rename` |

Multipart namespace components use the negotiated Iceberg U+001F separator on
the wire. Handlers decode the component array before constructing an internal
identifier. Rename already receives source and destination component arrays in
its JSON body and preserves them directly.

## Create is metadata-durable before pointer admission

A schema-based `createTable` request produces valid initial Iceberg metadata
with a fresh UUID, no snapshots, the requested schema/properties, and a stable
table location. LakeCat writes that metadata document create-only through the
selected storage profile before it admits the catalog pointer.

This ordering establishes a useful invariant:

```text
successful create response
        => returned metadata-location exists
        => the exact metadata body can be loaded or registered
```

If catalog admission fails after the object write, LakeCat removes only the
uncommitted object with bounded cleanup. It never overwrites an existing object
to make a duplicate create appear successful. Duplicate table identity is a
spec-shaped HTTP/code 409 `AlreadyExistsException`.

When the standard request carries `location`, LakeCat requires that location to
belong to the warehouse's selected storage profile. The metadata document and
all later commits retain that table location. When the request omits it,
LakeCat derives a location from its configured warehouse/default policy.

## Storage-profile boundary

Object access is selected from management-plane storage profiles, not from
credentials embedded in a client URI. Locations must be absolute, undecorated,
free of username/password material, and contained by the selected profile.
S3 profiles can use the standard object-store environment or a governed secret
reference. Secret values never enter catalog history, graph facts, OpenLineage,
or HTTP diagnostics.

For the C1-05 fixture, an operator registered a governed S3 profile rooted at:

```text
s3://warehouse/lakecat
```

The runner derived unique namespace/table child locations. It required table
location stability on create, load, update, rename, and registration, then used
an independent MinIO client to prove all three distinct referenced metadata
objects existed.

## List and load

Table collection reads first prove the parent namespace exists. Listing under an
absent namespace returns HTTP/code 404 with a nonempty
`NoSuchNamespaceException`; it does not collapse missing scope into an empty
inventory. Loading an absent or governed-hidden table returns HTTP/code 404 with
`NoSuchTableException`.

List results contain only active table identifiers in the exact requested
namespace. LakeCat currently uses the Iceberg-permitted complete unpaginated
fallback for table listing: it returns all unique identifiers and no continuation
token. The C1-05 runner requests a one-item page, verifies completeness and
uniqueness, and records the fallback explicitly rather than pretending it
observed pagination.

Load preserves the table UUID, table location, metadata location, schema,
properties, snapshot state, and other Iceberg metadata from the current admitted
pointer. Business semantics, policies, graph facts, and lineage do not become
required custom fields in that metadata.

## Updates and no-current-snapshot state

The standard table update route applies Iceberg requirements and updates through
the Sail-facing metadata engine, writes a new immutable metadata document, and
advances the catalog pointer only after validation and storage succeed. A
property update preserves the UUID, schema, table location, and unmentioned
properties while producing a new metadata location.

Iceberg represents a valid newly created table with no current snapshot as:

```json
{"current-snapshot-id": -1}
```

That is a wire/metadata sentinel, not a negative snapshot identity. LakeCat's
internal commit evidence has an established zero-valued no-snapshot
representation. The boundary therefore:

- normalizes exactly `-1` to zero when generating durable commit evidence;
- decodes a legacy serialized `-1` record as zero;
- rejects every other negative snapshot value;
- validates the generated history record before memory or Turso persistence;
  and
- stages the next memory value before mutation, so validation failure cannot
  leave table state and history inconsistent.

The rule matters beyond one commit. Before the correction, a property-only
update on a no-snapshot table persisted `-1`; a later rename revalidated history
and returned HTTP 500. Shared memory/Turso regressions now prove create,
property update, history decode, failure atomicity, and rename after a
no-snapshot commit.

Commit requirements, stale metadata-pointer rejection, exact request retry, and
same-idempotency-key content drift receive their catalog-neutral acceptance in
C1-06. Their absence from C1-05 is a scenario boundary, not a claim that every
case is already proven.

## Non-purging drop and namespace retirement

The standard table drop is governed and soft-deletes catalog visibility while
its namespace remains. `purgeRequested=false` removes the active pointer but
retains metadata objects, permitting management-plane recovery and standard
metadata registration. A subsequent load is a spec-shaped table 404.

An otherwise-empty namespace can then be dropped. Memory and Turso
transactionally retire hidden registrations and mutable table pointer-log and
idempotency state while preserving immutable audit/outbox history. Recreating
the namespace cannot collide with or restore an earlier hidden lifecycle.
Active tables, child namespaces, views, or relevant policy bindings still block
namespace drop.

## Standard metadata registration

`registerTable` accepts the standard name, metadata-location, and overwrite
shape. LakeCat performs the operation in this order:

1. resolve and authorize the destination as `table.register`;
2. prove the namespace exists;
3. reject `overwrite=true` before object access;
4. parse and scope-check the metadata URI without exposing credentials;
5. read at most 64 MiB through the object-store seam;
6. deserialize valid Iceberg metadata with a nonempty table location;
7. admit a new catalog identity without changing the metadata UUID, body, or
   metadata location; and
8. record `table.registered` audit/outbox evidence.

Duplicate destination identity remains 409. Missing namespace or metadata,
malformed/decorated URI, oversized metadata, invalid Iceberg JSON, and
out-of-profile storage fail before partial catalog admission. Registration is
currently false-overwrite only; unsupported overwrite is explicit rather than a
non-atomic pointer replacement disguised as support.

Registration emits its own lifecycle evidence rather than pretending to be a
new physical table create. Grust projection reuses the created-object graph
shape, and OpenLineage records the table-created vocabulary, but the admitted
catalog event remains `table.registered` with a matching authorization receipt.

## Standard rename

`renameTable` receives complete source and destination Iceberg identifiers. The
operation is authorized as `table.rename` against both scopes before mutation.
LakeCat supports destination namespaces inside the same served warehouse;
cross-warehouse movement is rejected because it would require a different
storage/governance transaction.

Memory and Turso perform one atomic transition that:

- verifies source visibility and destination namespace existence;
- rejects an occupied destination with 409;
- preserves metadata body, metadata pointer, UUID, version, and creation stamp;
- moves the active table identity;
- retargets commit-history scope and exact table-scoped policy rows;
- retires source-name-bound idempotency replays; and
- appends one immutable `table.renamed` event binding both identities.

Historical audit/outbox envelopes remain immutable. Grust receives a dedicated
`Renamed` action. OpenLineage receives the source dataset as input and the
destination dataset as output, expressing one transformation rather than two
misleading delete/create events.

Retiring name-bound replay state is deliberate. An old idempotency key must not
silently replay a response under a different REST identity after rename.

## Governance and evidence

Every mutation resolves the effective principal and asks the governance seam for
a typed decision before storage mutation. Table actions include create, update,
drop, restore, register, and rename. Admitted state changes append catalog audit
and outbox records transactionally with the pointer transition.

Outbox replay validates the action/receipt pairing before projecting to Grust or
OpenLineage. Table metadata remains standard Iceberg metadata; policy receipts,
semantic graph state, lineage, and QueryGraph handoff data remain control-plane
evidence. This keeps ordinary clients interoperable and lets downstream systems
verify attributable work independently.

## Error and atomicity contract

The public distinction is intentional:

| Condition | Required response |
| --- | --- |
| Missing namespace for list/create/register/rename destination | 404 `NoSuchNamespaceException` |
| Missing or hidden table | 404 `NoSuchTableException` |
| Duplicate create or occupied rename/register destination | 409 `AlreadyExistsException` |
| Stale pointer/commit requirement mismatch | conflict response owned by commit semantics |
| Unsupported registration overwrite | explicit client error before object access |
| Invalid or out-of-scope metadata location | bounded client error with secret-safe diagnostics |
| Internal storage/state failure | no partially admitted pointer transition |

Memory stages complete next values before replacing state. Turso validates and
mutates inside transactions. Object writes occur create-only before pointer
admission, and failed admission performs bounded cleanup without replacing the
authoritative error. Graph and lineage sinks consume admitted outbox state; they
do not sit inside the catalog compare-and-swap.

## Accepted C1-05 evidence

The optimized Linux ARM64 run used stable Rust 1.97.1, full
`turso-local,sail-local` LakeCat features, one Docker network, and one local
MinIO bucket. LakeCat passed all 15 required assertions plus optional rename and
registration. Gravitino, Lakekeeper, and Polaris did the same. Nessie passed 14
required assertions and both optional operations; only missing-namespace table
listing returned HTTP 200 instead of the required 404.

All five fixtures were fully reconciled, all transcripts passed sanitization,
and an independent client found every distinct referenced metadata object in
MinIO. This is behavior evidence, not timing evidence. The transcripts remain
ignored smoke artifacts until C1-09 materializes a runnable profile, immutable
result bundle, generated report, manual redaction review, and public site.

Exact identities and hashes are in
[`PHASE-1-ACCEPTANCE.md`](catalog-community/PHASE-1-ACCEPTANCE.md#c1-05--table-behavior).
Deterministic requirement admission, stale-state rejection, pointer atomicity,
and config-gated retry evidence are specified separately in
[`ICEBERG-COMMITS.md`](ICEBERG-COMMITS.md).

## Verification

The implementation is protected at three layers:

- `lakecat-api` tests preserve standard request/response wire shapes and endpoint
  advertisements;
- shared `lakecat-store` tests exercise memory and Turso lifecycle, atomicity,
  commit history, policy retargeting, idempotency retirement, and outbox state;
  and
- `lakecat-service` tests exercise routes, multipart identifiers, storage scope,
  bounded metadata reads, redaction, errors, registration, rename, graph/
  OpenLineage replay, and complete cleanup.

Use stable Rust only. The broad local gates are:

```sh
cargo fmt --all -- --check
cargo test -p lakecat-store --features turso-local
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Public workflow or acceptance changes also require the unified book build and
`scripts/check-book-artifact-contract.sh`.
