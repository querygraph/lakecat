# Iceberg REST Namespace Behavior

This document is the operator and contributor contract for LakeCat's Apache
Iceberg REST namespace surface. Revision
`c821a0dcb4b326c23f4a56472a2a5e574ef33fea` is the first implementation
accepted by the catalog-neutral C1-04 namespace scenario across the production
`turso-local,sail-local` build.

The acceptance evidence is recorded in
[`catalog-community/PHASE-1-ACCEPTANCE.md`](catalog-community/PHASE-1-ACCEPTANCE.md).
The harness, exact five-catalog matrix, and reproduction command are documented
in the
[catalog-bench namespace report](https://github.com/querygraph/catalog-bench/blob/b149ee74d574d13c51a894d9f440606be2b5a0c1/docs/NAMESPACE-CONFORMANCE.md).

## Standard routes

LakeCat exposes both its configured-warehouse compatibility routes and the
standard warehouse-prefixed forms. The request and response shapes are the same:

| Operation | Compatibility route | Warehouse-prefixed route | Success |
| --- | --- | --- | --- |
| Create | `POST /catalog/v1/namespaces` | `POST /catalog/v1/{warehouse}/namespaces` | HTTP 200 `NamespaceResponse` |
| List | `GET /catalog/v1/namespaces` | `GET /catalog/v1/{warehouse}/namespaces` | HTTP 200 `ListNamespacesResponse` |
| Load | `GET /catalog/v1/namespaces/{namespace}` | `GET /catalog/v1/{warehouse}/namespaces/{namespace}` | HTTP 200 `NamespaceResponse` |
| Update properties | `POST /catalog/v1/namespaces/{namespace}/properties` | `POST /catalog/v1/{warehouse}/namespaces/{namespace}/properties` | HTTP 200 `UpdateNamespacePropertiesResponse` |
| Drop | `DELETE /catalog/v1/namespaces/{namespace}` | `DELETE /catalog/v1/{warehouse}/namespaces/{namespace}` | HTTP 204 |

`NamespaceResponse.properties` and all other map-typed Iceberg fields serialize
as JSON objects. Create preserves the supplied string properties and load returns
the current durable map.

## Multipart identifiers and hierarchy

JSON request and response bodies represent a namespace as an ordered array of
components, for example `["accounting", "tax"]`. Iceberg
REST path and `parent` values join those components with the protocol's default
U+001F unit separator, percent-encoded as `%1F` in a URL:

```text
JSON identity:       ["accounting", "tax"]
resource path:       accounting%1Ftax
top-level parent:    accounting
```

LakeCat decodes U+001F only as the multipart separator. A dot remains part of a
single wire component; it is not the REST hierarchy delimiter. Empty or invalid
components are rejected by the shared `Namespace` type.

An unscoped list returns only top-level namespaces. `parent=<identifier>` first
loads that exact parent and returns only immediate children whose component
prefix matches it. Descendants more than one level below the parent are not
flattened into the response. An absent parent returns HTTP 404 with
`NoSuchNamespaceException` instead of an empty HTTP 200 page.

Drop is child-safe. A namespace with a descendant namespace, table, view, or
policy binding returns HTTP 409 and remains intact. Clients must remove dependent
objects and child namespaces before dropping the parent.

## Pagination

The list query accepts Iceberg's camel-case `pageToken` and `pageSize` fields.
LakeCat sorts namespaces by their component-wise `Namespace` order before
pagination, so unchanged catalog state yields deterministic traversal.

- Omitting both fields returns the complete matching list and no next token.
- Supplying either field enables pagination. An absent or empty token starts at
  offset zero.
- The default requested page size is 1,000 and the server caps it at 10,000.
- `pageSize=0`, malformed tokens, and offsets beyond the current result set
  return HTTP 400.
- A nonterminal page returns an opaque token with the current
  `lakecat-v1:<offset>` encoding. Clients must treat that encoding as private
  and send it back unchanged.
- The terminal page omits `next-page-token`.

Tokens are offsets into the current sorted list, not snapshots. A client that
requires a transactionally stable inventory must prevent concurrent namespace
mutation for the duration of its multi-request traversal.

## Durable component identity

The dot-joined namespace path is for human-readable display and compatibility;
it is not a durable database identity. LakeCat persists a versioned,
length-prefixed component encoding so a literal `["a.b"]` namespace cannot
alias multipart `["a", "b"]`. Turso applies that encoding consistently to
namespace, table, view/receipt, soft-delete, and policy scope.

On first startup after upgrade, LakeCat validates typed JSON against every
legacy row, rewrites all dependent keys in one transaction, and records a
schema marker only after success. A corrupt or inconsistent row aborts and
rolls back the entire migration. Subsequent starts skip the completed migration.

## Property updates

The update body contains `removals: string[]` and `updates: object`. The store
applies the operation functionally to the prior property map and returns three
deterministically ordered lists:

- `updated`: keys written from `updates`;
- `removed`: requested keys that existed and were removed; and
- `missing`: requested keys that did not exist.

Unmentioned properties remain unchanged. Empty property keys and duplicate
removal keys return HTTP 400. A key named in both `removals` and `updates`
returns HTTP 422 with `UnprocessableEntityException`; validation happens before
the store call, and the namespace property map remains unchanged.

## Error contract

All protocol failures use Iceberg's `ErrorModel` envelope with matching HTTP and
body codes:

| Condition | HTTP | Error type |
| --- | --- | --- |
| Malformed namespace, query, token, page size, or property update | 400 | `BadRequestException` |
| Missing namespace or list parent | 404 | `NoSuchNamespaceException` |
| Duplicate namespace | 409 | `AlreadyExistsException` |
| Non-empty namespace | 409 | `CommitFailedException` |
| Same property key removed and updated | 422 | `UnprocessableEntityException` |
| Store without optional property support | 501 | `UnsupportedOperationException` |

## Store semantics and migration

`CatalogStore` keeps namespace behavior backend-portable. Its compatibility
defaults support empty create/load properties and explicitly reject optional
property mutation when a backend has not implemented it.

The production stores implement the full contract:

- `MemoryCatalogStore` stores `Namespace -> NamespaceProperties` in one
  warehouse-scoped map. Create, update, dependency checks, and drop each hold
  the appropriate single store lock, so namespace identity and properties do
  not drift within memory state.
- `TursoCatalogStore` keeps the existing `namespaces` identity row and a
  `namespace_properties(warehouse, namespace_path, properties_json)` side
  table. Create writes both rows in one transaction. Update validates the
  namespace row's durable scope, reads and applies the old property map, then
  upserts the side row in one transaction. Drop validates descendants and all
  dependent object classes before deleting both rows in one transaction.
- A database created by an older LakeCat revision may have a namespace row with
  no property row. Load interprets that state as an empty map. The first
  successful property update lazily materializes the side row, avoiding a
  destructive eager rewrite of legacy state.

Turso reads bind decoded namespace JSON and property rows back to the selected
warehouse and namespace-path columns. Scope drift fails closed as an internal
integrity error rather than returning a valid-looking object under the wrong
identity.

## Governance, audit, graph, and lineage

Every handler authorizes before its store operation. Property mutation has a
dedicated typed `NamespaceUpdateCapability` backed by
`CatalogAction::NamespaceUpdate` and action name `namespace.update`; it does not
borrow create or load authority.

A successful property mutation records `namespace.properties-updated` with the
authorization receipt, warehouse, and namespace components. The audit payload
intentionally omits property keys and values. Turso pairs each accepted audit
row with its replayable outbox row, and the normal drain validates the receipt
action and namespace scope before projecting a Grust namespace upsert and an
OpenLineage `namespace-properties-updated` event. The audit/outbox pair is
transactional with itself; it is currently recorded after the namespace store
mutation rather than in the same cross-operation transaction.

Create, list, load, update, and drop retain their distinct authorization actions
and event types. List evidence records only the returned namespace paths and
count. No namespace property value is admitted to audit, outbox, graph, or
lineage replay evidence by these handlers.

## Verification

The implementation has separate protocol, service, security, and store tests.
Coverage includes:

- exact JSON shapes and the advertised property route;
- U+001F decoding while preserving a literal dotted wire component;
- deterministic immediate-child pagination and invalid-token rejection;
- create/list/load/update/drop, duplicate 409, missing-parent 404, overlap 422,
  and unchanged state after rejected overlap;
- memory/Turso property parity and parent-drop protection;
- Turso legacy-row lazy migration and durable scope validation; and
- redacted update audit evidence followed by successful outbox drain.

The accepted optimized Linux ARM64 build used stable Rust 1.97.1, release
optimization level 3, fat LTO, one codegen unit, stripped symbols,
`panic=abort`, disabled incremental compilation, `-Dwarnings`, and
`-Ctarget-cpu=native`, with features `turso-local,sail-local`. The same Docker
network and catalog-neutral runner tested LakeCat, Gravitino, Lakekeeper,
Polaris, and Nessie. LakeCat passed every required and optional C1-04 assertion.

## Known identity debt

The REST codec is component-correct, but the older internal
`Namespace::path()` representation joins components with `.`. Turso namespace
primary keys and several table, view, and policy-derived keys use that string.
Consequently, the single component `["a.b"]` can alias the multipart namespace
`["a", "b"]` in those internal key spaces even though the REST decoder keeps
them distinct.

C1-04 deliberately did not patch only one affected key family. A safe repair
requires a versioned, length-delimited or otherwise unambiguous canonical
component encoding; migration of every namespace-derived durable and in-memory
key; collision detection; compatibility reads; and cross-object regression
coverage. Until that migration lands, operators should not create literal-dot
components that collide with multipart identities. This limitation is tracked
as explicit C1-10 conformance debt.

The C1-04 files are sanitized smoke transcripts, not publishable benchmark
results. C1-09 still owns immutable result wrapping, exact environment capture,
manual redaction review, secret scanning, generated site/report output, and any
performance ranking.
