# LakeCat Review & Architecture Plan (OPUS1)

Review of `~/src/lakecat` (branch `master`, no commits yet — working tree only),
plus the integration targets it depends on: `~/src/sail`, `~/src/grust`,
`~/src/typesec`, and `~/src/querygraph`.

This document does two things the request asked for:

1. **Thoroughly review what Codex has built so far** — the scaffold, the
   architecture document, and the verified build/test state — with findings
   ordered by severity.
2. **Propose a refined architecture** for LakeCat as a Rust-first, Iceberg
   v1–v3 compatible (v4-ready) catalog that pushes as much work as possible into
   Sail, uses Grust for the semantic graph, TypeSec for governance and secure
   agents, and lands QueryGraph end-to-end as the goal.

The existing [`ARCHITECTURE.md`](../../ARCHITECTURE.md) is a strong north star. This
plan keeps its intent and corrects where the current code diverges from it.

---

## Verification notes

All commands run in this environment against the current working tree.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | **PASS** |
| `cargo test --workspace` (default features) | **PASS** |
| `cargo test --workspace --all-features` | **PASS** |
| `cargo test -p lakecat-service --all-features` | **PASS** — 8 tests |
| `cargo test -p lakecat-sail --features sail-local` | **PASS** — 10 tests |
| `cargo test -p lakecat-security --features typesec-local` | **PASS** — 1 test |
| `cargo test -p lakecat-graph --features grust-local` | **PASS** — 1 test |
| `cargo test -p sail-iceberg manifest_avro_round_trips_lower_and_upper_bounds` | **PASS** |
| `git diff --check` | **PASS** |

Key fact this surfaces: **the default and all-features gates are both green now.**
The meaningful local integrations still live behind feature flags, but the
default-feature test path no longer asserts `sail-local` behavior by accident.
The service has explicit `sail-local`, `typesec-local`, and `grust-local`
feature passthroughs.

Confirmed real (not stubbed) in sibling repos:

- Sail exposes `sail_catalog::provider::CatalogProvider` (async trait with
  `create_table`, `commit_table`, `get_table_commits`, scan/maintenance hooks) and
  `sail-catalog-iceberg/src/models/*` (full generated Iceberg REST OpenAPI model
  set). LakeCat genuinely reuses these.
- Grust exposes `grust::Graph`, `GraphStore::put_graph`, `GraphIndex`, and — via
  `grust-sail` — `SailGraphStore`, the Sail/DataFusion-backed graph store. The
  "Sail-backed graph storage as the production path" claim is realizable today.
- TypeSec exposes `PolicyEngine::check(&SubjectId, &str, &ResourceId) ->
  PolicyResult { Allow, Deny, Delegate }`, plus `RbacEngine`, `OdrlEngine`, and
  the `Capability<P, R>` type-level core. LakeCat's `typesec-local` bridge compiles
  and passes against the real crate.
- QueryGraph's `qg-rust` already has `sail.rs`, `croissant.rs`, `cdif.rs`,
  `odrl.rs`, `lineage.rs`, `qglake.rs`, `lakehouse.rs`, `agent.rs`, and a fully
  written end-to-end story in [`QGLake.md`](../../../querygraph/qg-rust/QGLake.md)
  (the "Resilience Desk"). That story is the natural acceptance test for LakeCat.

---

## Executive summary

Codex delivered a **well-shaped, trait-seamed scaffold** of ~6,100 Rust lines
across 9 crates, plus a thoughtful architecture document. The standout is the
Sail bridge: it does real Avro manifest-list/manifest reading, delete-file
indexing with inherited sequence/row-ids, conservative file-bounds pruning, and
round-trips every wire payload through Sail's generated Iceberg REST models. The
QueryGraph bootstrap projects live tables into Croissant + CDIF + OSI + ODRL +
OpenLineage in one content-hashed bundle.

The verdict: **the seams are right, the Iceberg-spec reuse is genuine, but the
catalog is not yet a catalog.** Three load-bearing pieces from the architecture
are absent or decorative:

- **No authentication and no real principal** — every endpoint hardcodes
  `Principal::anonymous()`, so governance is cosmetic.
- **No durable commit** — `commit_table` bumps an integer in an in-memory map; it
  never writes a metadata file, never does compare-and-swap on a metadata
  pointer, and never records an idempotency/audit log. There is no persistence
  backend at all (no Postgres/sqlite, despite the doc).
- **"Push work into Sail" is currently "reuse Sail's structs in-process,"** not
  delegation to Sail's planner/optimizer. LakeCat does the pruning itself using
  Sail's spec types; it never calls a Sail `CatalogProvider` or DataFusion plan.

None of that is wrong as a *starting* posture — it's a faithful Milestone 1–2
scaffold. But the architecture's headline promises (governed, idempotent,
auditable, Sail-planned) are not yet exercised. The plan below sequences the work
to close that gap while keeping the seams Codex already got right.

---

## What Codex delivered (crate by crate)

Total ≈ 6,102 lines. Workspace edition 2024, resolver 2.

| Crate | Lines | What's actually there |
| --- | --- | --- |
| `lakecat-core` | 226 | Typed IDs (`ProjectId`, `WarehouseName`, `Namespace(Vec<String>)`, `TableName`, `TableIdent`), `Principal`/`PrincipalKind` (Anonymous/Human/Service/Agent), `AuditStamp`, SHA-256 content hashing, name validation. Clean. |
| `lakecat-store` | 254 | `CatalogStore` async trait + **`MemoryCatalogStore` only**. Create/list namespace, create/load/list table, `commit_table`. No Postgres/sqlite. Commit mutates an in-memory record. |
| `lakecat-api` | 226 | Iceberg REST request/response DTOs incl. `PlanTableScanRequest/Response`, `FetchScanTasks*`. Scan-mode validation (snapshot vs incremental). Uses `serde_json::Value` for metadata passthrough. |
| `lakecat-sail` | 3,259 | The engine. `SailCatalogEngine` trait; `DeferredSailCatalogEngine` (default) and `SailRestModelCatalogEngine` (`sail-local`). Commit-requirement validation against typed `TableMetadata`; manifest-list/manifest scan-task planning; opaque plan-task tokens; `fetchScanTasks` expansion with delete-file refs; file-bounds pruning; v4 "extension mode" JSON path. ~All logic behind `sail-local`. |
| `lakecat-security` | 200 | `GovernanceEngine` trait + `AllowAllGovernanceEngine` + `TypeSecGovernanceEngine` (`typesec-local`). `CatalogAction` enum, `AuthorizationRequest/Receipt`. |
| `lakecat-graph` | 207 | `CatalogGraphSink` trait + `NoopCatalogGraphSink` + `GrustCatalogGraphSink` (`grust-local`). Emits a 2-node/1-edge "CatalogEvent → Table" graph per event. |
| `lakecat-lineage` | 76 | `LineageSink` trait + `HashOnlyLineageSink` (hashes the event, returns a receipt; no transport). |
| `lakecat-querygraph` | 537 | `QueryGraphBootstrap::from_tables` → per-table Croissant, CDIF, OSI, ODRL + a catalog graph + an OpenLineage COMPLETE event, all content-hashed. The broadest, most "finished" crate. |
| `lakecat-service` | 1,100 (incl. tests) | `axum` router, 8 endpoints under `/catalog/v1` + `/querygraph/v1/bootstrap`. `LakeCatState` wires store/sail/governance/graph/lineage. `main.rs` binds `127.0.0.1:8181` with the memory store and **default (placeholder) integrations**. |

Proposed-but-absent: **`lakecat-cli`** (in the architecture's workspace list, not
created), and the management/QueryGraph API surfaces beyond bootstrap.

### What is genuinely good

- **Trait seams with no-op defaults + feature-gated real impls.** Every external
  system (Sail, TypeSec, Grust, lineage) is a trait with a safe default and a
  real implementation behind a feature. This is exactly the right shape for a
  catalog that must run embedded *and* wired to heavy engines.
- **Real Iceberg-spec reuse, carefully done.** `lakecat-sail` reads Avro manifest
  lists and manifests through `sail_iceberg::io`, builds a `DeleteFileIndex`,
  inherits sequence numbers and `first_row_id`, and prunes data files against
  column bounds with *sound conservative* semantics: missing metrics keep the
  file; `NOT` over bounds keeps the file; string `starts-with` uses a correct
  next-lexicographic-prefix range. Commit validation covers all seven
  `TableRequirement` variants against typed metadata.
- **Wire conformance is tested, not assumed.** Payloads round-trip through
  `sail_catalog_iceberg::models` (`PlanTableScanRequest`, `FetchScanTasksResult`,
  `FileScanTask`, `PositionDeleteFile`, `CompletedPlanningWithIdResult`). This is
  the right way to keep Iceberg REST compatibility honest.
- **QueryGraph projection breadth.** One call yields Croissant + CDIF + OSI + ODRL
  + OpenLineage with stable IDs and a bundle hash — directly aligned with the
  QGLake demo's semantic-projection model.
- **Namespace identity as a validated component vector** (`Namespace(Vec<String>)`),
  exactly the LanceDB lesson the architecture cites.

---

## Findings (ordered by severity)

### 1. (HIGH) `cargo test --workspace` is red on default features — mis-gated tests

**File:** `crates/lakecat-service/src/lib.rs` — `create_load_commit_and_plan_table_round_trips_through_integrations` (~line 553) and `plan_rejects_invalid_incremental_scan_modes` (~line 735).

Both tests assert `sail-local`-engine behavior — e.g.
`payload["lakecat-plan-tasks"][0]["task-type"] == "manifest-list"` and that an
invalid incremental range returns `400` — but they are **not** gated behind
`#[cfg(feature = "sail-local")]`. With default features the active engine is
`DeferredSailCatalogEngine`, which returns empty scan tasks and ignores
incremental ranges, so the assertions fail (`Null != "manifest-list"`,
`200 != 400`).

**Fix:** either gate both tests behind `#[cfg(feature = "sail-local")]` (matching
the two that already are), or split each into a feature-agnostic part (status
codes that hold for both engines) and a `sail-local` part. Add a CI matrix that
runs `--workspace`, `--features sail-local`, `--features typesec-local`, and
`--features grust-local` so this can't regress. Until fixed, the default
`cargo test` gate is broken.

**Current status:** addressed in the current working tree by keeping the service
tests compatible with the active engine and verifying the all-features service
path. A CI matrix is still pending.

### 2. (HIGH) No authentication; every request is `Principal::anonymous()`

**File:** `crates/lakecat-service/src/lib.rs` — every handler constructs
`Principal::anonymous()` (lines 129, 178, 223, 258, 319, 392, …).

There is no auth middleware, no bearer/OAuth/OIDC token extraction, no mapping
from a credential to a `Principal` or TypeDID envelope. Consequently the
`GovernanceEngine` always authorizes an anonymous subject, and the `TypeSec`
bridge — even when enabled — can only ever see `SubjectId("anonymous")`. The
authorization layer is therefore decorative. Sail's `sail-catalog-iceberg`
already carries OAuth/bearer wiring that can be reused.

**Fix:** add a `tower` auth layer that resolves an `Authorization` header into a
typed `Principal` (+ optional TypeDID envelope) and injects it as a request
extension; thread it into every handler in place of `anonymous()`. See the
"Authentication & the capability model" section below for the typed approach.

**Current status:** partially addressed. Handlers now resolve typed principals
from `x-lakecat-principal`, `x-lakecat-principal-kind`,
`x-lakecat-agent-did`, and bearer authorization headers before calling the
governance engine. A full reusable `tower` middleware and TypeDID envelope
verification remain pending.

### 3. (HIGH) Commit does not persist Iceberg metadata or compare-and-swap a pointer

**Files:** `crates/lakecat-service/src/lib.rs::commit_table` (line 253);
`crates/lakecat-store/src/lib.rs::commit_table` (line 170).

The architecture specifies an optimistic, idempotent, auditable commit: validate
→ write new `metadata.json` via the storage profile → atomically CAS the table
metadata pointer → store idempotency + audit records → emit events. What the code
actually does:

- Sail validates requirements and echoes back updates (good, real).
- `MemoryCatalogStore::commit_table` increments an integer `version`, writes
  `metadata["lakecat:version"]` and `lakecat:last-request-hash` into the
  **in-memory** metadata blob, and appends a minimal `TableCommitRecord` to a
  `Vec` that is never read back.
- **No new metadata file is written**, `metadata_location` never changes, and the
  service passes `idempotency_key: None` (line ~286), so the idempotency map is
  dead code. There is no CAS, no `metadata_pointer_log`, no `audit_events`.

So Iceberg metadata is *not* the source of truth and the pointer is *not*
atomically swapped — the two core contracts of an Iceberg catalog.

**Fix:** introduce a real metadata-pointer store (Finding 4) and an
`object_store`-backed metadata writer; have `commit_table` (a) ask Sail to
assemble the new `TableMetadata`, (b) serialize and write `…/metadata/NNNNN.json`,
(c) CAS `(previous_metadata_location → new_metadata_location)` in the relational
store, (d) persist idempotency + audit + pointer-log rows, then (e) emit events.

### 4. (MEDIUM) No persistence backend exists (no durable Turso/Postgres store)

**Files:** `Cargo.toml` (workspace deps), `crates/lakecat-store`.

`ARCHITECTURE.md` specifies a durable catalog store and Lakekeeper-style tables
(`projects`, `warehouses`, `storage_profiles`, `namespaces`, `tables`, `views`,
`metadata_pointer_log`, `idempotency_records`, `soft_deletes`, `policy_bindings`,
`audit_events`). The full set does not exist. `object_store` is declared with
`aws`/`gcp`/`azure`/`http` features but is used only by `lakecat-sail` for a
`LocalFileSystem` reader. There are no projects, warehouses, storage profiles,
soft-delete, or views — the warehouse is a single hardcoded `"local"` in
`main.rs`.

**Fix:** implement a durable store behind the existing `CatalogStore` trait,
starting with Rust-native Turso for local durable demos/tests and keeping the
schema portable for remote Turso/libSQL or a later production backend. Keep
`MemoryCatalogStore` for tests/embedded. This is the spine the commit path
(Finding 3) hangs on.

**Current status:** partially addressed. `lakecat-store` now has an opt-in
`turso-local` feature with `TursoCatalogStore` covering namespaces, table
records, metadata pointer history, idempotency records, audit events, and outbox
events; the service binary can use it via `LAKECAT_TURSO_PATH`. The full
Lakekeeper-style management tables, object metadata writes, pointer CAS, and
outbox draining remain pending.

### 5. (MEDIUM) The service binary cannot activate the real TypeSec/Grust engines

**File:** `crates/lakecat-service/Cargo.toml` — `[features]` exposes only
`sail-local`. `main.rs` calls `LakeCatState::new(...)`, never `with_integrations`,
so it always runs `AllowAllGovernanceEngine` + `NoopCatalogGraphSink` +
`HashOnlyLineageSink`.

There is no `typesec-local` / `grust-local` passthrough in the service crate and
no runtime config to select engines. So even a fully built binary can't enforce
TypeSec policy or write a Grust graph without code edits.

**Fix:** add `typesec-local` and `grust-local` passthrough features to
`lakecat-service`, plus a small config (env/CLI) that builds the chosen engines
and calls `with_integrations`. This is also where `lakecat-cli` should live.

**Current status:** partially addressed. `lakecat-service` now exposes
`typesec-local` and `grust-local` passthrough features and the binary wires local
TypeSec/Grust integrations behind those feature gates. Runtime config and
`lakecat-cli` remain pending.

### 6. (MEDIUM) Sail is used as a struct library, not as a planner

**File:** `crates/lakecat-sail/src/lib.rs` (`sail_integration` module).

The bridge reuses `sail-iceberg` *spec + IO* and `sail-catalog-iceberg` *models*,
and then does manifest expansion and file pruning **inside LakeCat**. It never
constructs a Sail `CatalogProvider`, never builds a DataFusion plan, and the
deferred residual (`"limit-deferred-to-sail"`, projection, filters) is returned to
the client but consumed by nothing. So today's reality is "LakeCat prunes using
Sail's data structures," which is good engineering but is *not* the architecture's
"Sail remains the optimizer; LakeCat is the policy facade."

**Fix:** define the Sail integration as three explicit tiers (see "Sail
integration tiers" below) and pick which one each endpoint targets. The
near-term, highest-leverage move is Tier 1: implement Sail's `CatalogProvider`
for LakeCat so Sail/DataFusion can scan LakeCat tables in-process — that is the
real "bring the engine to the data."

### 7. (MEDIUM) Plan ↔ implementation drift

The architecture and the code disagree in several concrete places; pick one and
align (the doc is the better target):

| Architecture says | Code does |
| --- | --- |
| Stable IDs `lakecat:namespace:{warehouse}:{path_hash}`, `…:scan-plan:{sha256}`, `…:commit:{table}:{seq}:{sha256}` | `TableIdent::stable_id` = `lakecat:table:{warehouse}:{namespace}:{name}`; no path-hash, scan-plan, or commit IDs |
| Entity model `Server › Project › Warehouse › Namespace › Table/View` | Only `Warehouse`(hardcoded) › `Namespace` › `Table`; no Server/Project/View entities |
| TypeSec actions incl. `table.register`, `table.drop`, `graph.read`, `lineage.read` | `CatalogAction` has 8 variants; register/drop/graph/lineage absent |
| Rich Grust node/edge taxonomy (18 node labels, 14 edge labels) | Grust sink emits a generic `CatalogEvent`+`Table`+`EMITTED` triple |
| Authorization receipt persisted with audit event + attached to lineage | Receipt is checked for `.allowed` then dropped |
| `lakecat-cli` crate | absent |

**Fix:** treat the architecture as the contract; implement the entity model and
stable-ID scheme in `lakecat-core`, expand `CatalogAction`, and make the Grust
sink emit the typed taxonomy (Finding 8).

### 8. (MEDIUM) Grust graph emission is a placeholder, not the catalog graph

**File:** `crates/lakecat-graph/src/lib.rs::grust_integration` (line 123).

`graph_event_to_grust` writes a `CatalogEvent` node and (optionally) a `Table`
node with one `EMITTED` edge. The architecture's value is the *semantic* graph:
Namespace `CONTAINS` Table `HAS_COLUMN` Column, Table `CURRENT_SNAPSHOT` Snapshot
`HAS_MANIFEST` Manifest `HAS_DATA_FILE` DataFile, Table `GOVERNED_BY` Policy,
Principal `CAN_PLAN`/`CAN_COMMIT` Table, ScanPlan/Commit/LineageRun provenance.
None of that structure is built, so a graph consumer (QueryGraph) gets event
breadcrumbs, not a queryable catalog graph.

**Fix:** define a typed Grust schema for the taxonomy and translate each catalog
transition into the corresponding node/edge upserts. Prefer the `grust-sail`
`SailGraphStore` so the graph itself becomes lakehouse data (closing the loop),
and emit through an outbox (Finding 10) so graph writes never block a commit.

**Current status:** boundary clarified. Reusable catalog graph taxonomy,
projection builders, stores, and traversal/query behavior should be implemented
in Grust, then called from LakeCat through its thin graph sink.

### 9. (LOW) `MemoryCatalogStore::list_namespaces` fabricates a `default` namespace

**File:** `crates/lakecat-store/src/lib.rs` line ~124. When a warehouse has no
namespaces, it returns `vec![Namespace::root_default()]` — a namespace nobody
created. This makes "list after create" inconsistent and would surprise an
Iceberg client.

**Fix:** return an empty list; create `default` explicitly at warehouse
provisioning if desired.

**Current status:** addressed in `MemoryCatalogStore`; an empty warehouse now
lists no namespaces.

### 10. (LOW) Side effects are coupled to the request path

**File:** `crates/lakecat-service/src/lib.rs` — handlers do
`state.graph.emit(...).await?` and `state.lineage.emit(...).await?` inline, after
the store mutation, propagating any sink error as a request failure. A flaky
graph/lineage sink would fail an otherwise-successful catalog operation, and the
emit happens outside any transaction with the store write.

**Fix:** persist a transactional outbox row in the same store transaction as the
state change, and drain it asynchronously to Grust/lineage. This also gives
at-least-once delivery and replay (the architecture's "replay/export" goal).

### 11. (LOW) Plan-task tokens are unauthenticated and leak physical paths

**File:** `crates/lakecat-sail/src/lib.rs` — `opaque_plan_task*` /
`decode_plan_task`. Tokens are either `lakecat:sail:{kind}:{snapshot}:{path}` or
`lakecat:sail-json:{hex(JSON)}`; both embed absolute object-store paths (and, in
the JSON form, the filter expressions) and are neither signed nor encrypted. A
client can craft a token pointing at an arbitrary manifest path.
`validate_decoded_plan_task` re-checks the path against the snapshot's manifest
list for typed (v1–v3) metadata, which mitigates the typed path; the v4 path
validates more weakly.

**Fix:** treat plan tasks as opaque server state — either sign them (HMAC over
`{table, snapshot, path, filters, principal, expiry}`) or store them server-side
keyed by an opaque id. Never trust a decoded path without re-validation.

**Current status:** partially addressed. Newly emitted plan-task tokens are
structured and include the planned table stable ID; fetch rejects cross-table
replay and still revalidates manifest paths against typed metadata. HMAC or
server-side opaque token storage remains pending.

### 12. (LOW) v4 "ready" rests on JSON passthrough with thin coverage

**File:** `crates/lakecat-sail/src/lib.rs` — `inspect_v4_extension_metadata`,
`v4_extension_manifest_plan_tasks`. Format-version 4 bypasses typed
`TableMetadata::from_json` (only v1–3 parse), so v4 is hand-rolled JSON
inspection: incremental scans are rejected, projection/filter column validation
is skipped (`v4_extension_mode` short-circuits), and pruning is unavailable. This
is a reasonable placeholder, but the "v4-ready" posture currently means "v4 JSON
flows through without typed guarantees," which is worth stating plainly and
covering with round-trip tests as the spec settles.

**Fix:** keep v4 behind the capability flag (already done); add v4 round-trip
fixtures; converge on typed v4 metadata once `sail-iceberg` gains it, rather than
maintaining a parallel JSON inspector long-term.

---

## Proposed architecture (refined)

The shape in `ARCHITECTURE.md` is correct. The refinements below make the
"push work into Sail," governance, and persistence promises real, and aim the
whole thing at QueryGraph.

```text
   Spark / Trino / Flink / PyIceberg          QueryGraph agents (TypeDID)
            │  Iceberg REST                              │  /querygraph/v1
            ▼                                            ▼
   ┌───────────────────────────────── LakeCat service (axum/tower) ─────────────┐
   │  auth layer → Principal + TypeDID envelope                                  │
   │  TypeSec governance → Capability<Action, Resource> (the proof)             │
   │  catalog API (/catalog/v1)         management API (/management/v1)         │
   │  ── thin: identity, tenancy, API compat, policy gate, event outbox ──      │
   └───────┬───────────────────┬───────────────────┬───────────────┬───────────┘
           │ CatalogStore       │ SailCatalogEngine  │ GraphSink      │ LineageSink
           ▼                    ▼                    ▼               ▼
   Postgres/sqlite        Sail (3 tiers)        Grust (SailGraphStore)  OpenLineage
   + object_store         CatalogProvider /      = lakehouse graph      + TypeDID
   (metadata pointer,     DataFusion planning    (grust_nodes/edges)    attestation
   CAS, idempotency,      / remote scan-plan
   audit, soft-delete)
           └──────────────── object storage: Iceberg metadata + data ───────────┘
```

### Boundary: thin service, Sail-heavy engine (unchanged, reaffirmed)

LakeCat keeps only: identity & tenancy, Iceberg REST + management API
compatibility, the policy gate, atomic pointer state, and the event outbox.
Everything that touches data or table semantics is Sail's job. The current code
honors this seam; the work is to make the Sail side a planner, not a struct
library.

### Sail integration tiers (make Finding 6 concrete)

- **Tier 0 — spec & model reuse (done).** Use `sail-iceberg` spec/IO and
  `sail-catalog-iceberg` models for validation, manifest reading, and wire
  conformance. Keep this; it's the compatibility floor.
- **Tier 1 — LakeCat *is* a Sail catalog (next).** Implement
  `sail_catalog::provider::CatalogProvider` for LakeCat (it already defines
  `commit_table` / `get_table_commits` hooks). Then Sail's DataFusion engine can
  resolve, plan, and scan LakeCat tables **in-process**, which is the literal
  "bring the engine to the data." LakeCat's REST `plan`/`fetch-scan-tasks` then
  lower to a Sail logical plan instead of hand-rolled manifest walking.
- **Tier 2 — remote scan-planning service.** Expose Sail planning as a callable
  entrypoint (in `sail-plan-lakehouse`) that takes `(metadata pointer,
  projection, filter, snapshot | incremental range)` and returns DataFusion-pruned
  Iceberg file-scan tasks. LakeCat authenticates + authorizes + records, and
  forwards to Sail. This is where the catalog stops being a passive pointer
  service.

Each REST endpoint declares which tier it uses; the migration is incremental and
testable because Tier 0 already validates the wire types.

### Authentication & the capability model (Findings 2, 5)

Adopt TypeSec's central idea — *the capability is the proof* — rather than a
boolean receipt:

1. An auth `tower` layer resolves the credential into a `Principal`
   (`Human`/`Service`/`Agent`) and, for agents, a TypeDID envelope.
2. The governance engine returns not just `allowed: bool` but a minted
   `Capability<Action, Resource>` (TypeSec `Capability<P, R>` is unforgeable —
   only the engine constructs it). Handlers that perform privileged work *require*
   the capability type in their signature, so an unauthorized path doesn't
   compile/exist. The boolean `AuthorizationReceipt` becomes the audit artifact,
   persisted with the `audit_events` row and attached to lineage (closing the
   drift in Finding 7).
3. Compose engines as TypeSec intends: `OdrlEngine` decides usage constraints and
   `Delegate`s uncovered actions to `RbacEngine`. Expand `CatalogAction` to the
   full set (`table.register`, `table.drop`, `graph.read`, `lineage.read`, …).

### Persistence & commit (Findings 3, 4)

- `TursoCatalogStore` implementing `CatalogStore` exists for local durable
  namespaces, table records, metadata pointer history, idempotency records,
  audit events, and outbox events. The remaining management tables, object
  metadata writes, pointer CAS, and outbox draining are still pending.
  `object_store` writes the metadata files.
- Commit becomes: Sail assembles new `TableMetadata` → write
  `…/metadata/NNNNN.json` → CAS the pointer row `(prev → new)` in one transaction
  with the idempotency + audit + pointer-log + outbox rows → return. Iceberg
  metadata stays the source of truth; the relational store is only the atomic
  pointer and management state (the Iceberg catalog contract).
- Borrow from **Lakekeeper**: separate catalog API from management API;
  warehouses + storage profiles + credential vending as first-class objects;
  never share warehouse locations across tenants; prefer external IdPs; soft
  delete + restore; optional event sinks (the outbox).
- Borrow from **LanceDB**: namespace identity as validated component vectors
  (done), backend-independent namespace API, embedded/local-first mode (memory +
  sqlite), and AI-first catalog surfaces (Croissant/OSI — done).

### Graph: Grust as the catalog's semantic index (Finding 8)

- Define a typed Grust schema for the full node/edge taxonomy and translate each
  catalog transition into node/edge upserts (Namespace→Table→Column,
  Table→Snapshot→Manifest→DataFile/DeleteFile, Table→Policy, Principal→Table
  capabilities, ScanPlan/Commit/LineageRun provenance).
- Production store = `grust-sail` `SailGraphStore`, so the catalog graph is itself
  Iceberg/lakehouse data queryable by Sail — the graph and the data share one
  engine. Memory store for tests.
- Write through the outbox so graph mutation never blocks a commit.

### Lineage & attestation (Finding 10)

Replace `HashOnlyLineageSink` with a real OpenLineage transport (HTTP/file) plus
TypeDID attestation: each catalog change, scan plan, commit, and agent answer
emits an OpenLineage event and a compact signed DID root, persisted with the
audit event and surfaced to QueryGraph. This matches the QGLake provenance model
exactly.

### QueryGraph as the integration target (the goal)

QueryGraph's `qg-rust` already implements `sail.rs`, `croissant.rs`, `cdif.rs`,
`odrl.rs`, `lineage.rs`, `qglake.rs`. LakeCat should become the catalog substrate
under those modules rather than a parallel system:

- `lakecat-querygraph` already emits the Croissant/CDIF/OSI/ODRL/OpenLineage
  bundle; wire `querygraph import-lakecat --catalog … --warehouse … --build-bundle
  --load-graph --verify-policy` to consume it.
- The **QGLake "Resilience Desk"** becomes LakeCat's end-to-end acceptance test:
  a supervisor agent delegates to compartmentalized specialists that plan scans
  *through LakeCat*, each gated by TypeSec/ODRL, each scan recorded via
  OpenLineage + DID, and a synthesis agent aggregates only signed summaries —
  with the `RestrictedDataBroker` exercising metadata-visible / data-denied
  credential vending. If that demo runs green against LakeCat, the architecture is
  proven.

---

## What to push upstream into Sail

Per the architecture's "What Belongs In Sail," and validated against the actual
crates:

- Shared Iceberg REST models + conversion helpers (already in
  `sail-catalog-iceberg`; LakeCat should depend, not fork — it does).
- Catalog-managed commit assembly + idempotency-key support, exposed on
  `CatalogProvider::commit_table`.
- ETag/freshness-aware metadata loading.
- A remote scan-planning entrypoint (Tier 2) in `sail-plan-lakehouse`:
  request/response models + table-scan lowering that returns pruned Iceberg file
  tasks.
- Metadata-as-data plans (manifests, partition/file stats, delete indexes) and
  table-maintenance primitives (expire snapshots, rewrite manifests, compact,
  compute stats) as `CatalogProvider`/planner extension traits an external
  catalog can call without depending on LakeCat internals.

LakeCat must not reimplement manifest pruning, delete application, or
physical-plan construction. (Today it does some pruning in-process via Sail spec
types — acceptable as Tier 0, but the Tier 1/2 move is to hand that to Sail.)

---

## Milestones (ordered)

0. **Make `cargo test --workspace` green.** Gate the two mis-gated service tests
   (Finding 1) and add the CI feature matrix. Cheap, unblocks everything.
1. **Auth + typed principal** (Finding 2): `tower` auth layer →
   `Principal` (+ TypeDID for agents) threaded through all handlers; persist the
   authorization receipt.
2. **Persistence spine** (Finding 4): `TursoCatalogStore` first, with migrations
   for the Lakekeeper-style tables; keep `MemoryCatalogStore` for tests.
3. **Real commit** (Finding 3): Sail-assembled metadata → `object_store` write →
   pointer CAS → idempotency/audit/pointer-log/outbox in one transaction.
4. **Wire real engines into the binary** (Finding 5): `typesec-local` +
   `grust-local` passthrough features, runtime config, `lakecat-cli`.
5. **TypeSec capability model** (governance): mint `Capability<Action,Resource>`;
   ODRL→RBAC delegation; expand `CatalogAction`.
6. **Sail Tier 1** (Finding 6): implement `CatalogProvider` for LakeCat; lower
   `plan`/`fetch-scan-tasks` through Sail.
7. **Grust catalog graph** (Finding 8): typed taxonomy via `SailGraphStore`,
   emitted through the outbox.
8. **Lineage + DID** (Finding 10): OpenLineage transport + TypeDID attestation.
9. **QueryGraph end-to-end**: `querygraph import-lakecat`; run the QGLake
   Resilience Desk demo against LakeCat as the acceptance test.
10. **Sail Tier 2 + v4**: remote scan-planning entrypoint upstreamed to Sail;
    typed v4 metadata and round-trip tests as the spec settles.

---

## Non-goals (kept from the architecture)

- Do not invent a LakeCat table format.
- Do not bypass Iceberg metadata semantics for speed.
- Do not embed business semantics into Iceberg metadata as the only source of
  truth.
- Do not make LakeCat own graph algorithms (use Grust) or agent security (use
  TypeSec).
- Do not make QueryGraph depend on non-standard catalog endpoints for normal
  Iceberg table access.

---

### One-line verdict

A faithful, well-seamed Milestone-1 scaffold with genuinely careful Iceberg-spec
reuse and a broad QueryGraph projection — but not yet a catalog: no auth, no
durable/CAS commit, no persistence, and Sail used as a struct library rather than
a planner. Fix the red tests, add the auth + persistence + real-commit spine,
promote Sail to Tier 1, and drive the QGLake demo end-to-end.
