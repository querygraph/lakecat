# LakeCat Design Assessment (OPUS1-DESIGN)

A design-level companion to [`OPUS1.md`](OPUS1.md). Where OPUS1 reviews *what
Codex built*, this answers a sharper question:

> Can we achieve innovative QueryGraph features while conforming to Iceberg v3 (and
> soon v4) **and** bringing Sail closer to the data?

Short answer: **yes — and not by accident.** The three goals look like they're in
tension, but Iceberg's own evolution is handing us the exact seams these features
need. The design's core bet is right. What follows is why, where the real friction
actually lives (it is not where it looks), and two concrete sketches: the dual
read-path, and the Tier‑1 Sail integration drafted against Sail's real trait.

---

## Thesis

Iceberg compatibility constrains **one layer**: how table state is stored
(metadata.json, manifests, snapshots) and how external engines talk to you (the
REST protocol). It says almost nothing about what you *compute* from that
metadata, what you *record alongside* it, how you *govern* it, or what happens in
the catalog *control plane*. Every QueryGraph innovation — semantic graph,
governance, agents, lineage, AI-first projections — lives in those unconstrained
layers. So the architecture's central move is the only sustainable one:

> **Keep Iceberg semantics pristine at the boundary; innovate in the control plane
> and the engine.**

The corollary, which is the whole game: the innovation must live where *standard
engines can't bypass it*, and must rest on the *version-stable* substrate so it
survives v3 → v4.

---

## Why the goals don't actually conflict

Think of LakeCat as three concentric layers with a hard compatibility floor:

```text
┌─────────────────────────────────────────────────────────────────────┐
│  DERIVED / SEMANTIC  (free to innovate, derived from committed state) │
│    Grust catalog graph · Croissant/CDIF/OSI · ODRL · OpenLineage+DID  │
│    — never a parallel source of truth; always a projection           │
│  ┌───────────────────────────────────────────────────────────────┐   │
│  │  CONTROL PLANE  (free to innovate, governs access)            │   │
│  │    identity · TypeSec capabilities · tenancy · policy gate ·  │   │
│  │    pointer CAS · idempotency · audit · event outbox          │   │
│  │  ┌─────────────────────────────────────────────────────────┐ │   │
│  │  │  ENGINE  (Sail: planning, pruning, metadata-as-data)    │ │   │
│  │  │  ┌───────────────────────────────────────────────────┐  │ │   │
│  │  │  │  ICEBERG COMPATIBILITY FLOOR (must not corrupt)   │  │ │   │
│  │  │  │  metadata.json · manifests · snapshots · REST API │  │ │   │
│  │  │  └───────────────────────────────────────────────────┘  │ │   │
│  │  └─────────────────────────────────────────────────────────┘ │   │
│  └───────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

The graph is a *projection of committed Iceberg state*, never a second truth. The
explicit non-goal — "don't make Iceberg metadata the only home for business
semantics" — is the load-bearing discipline that keeps the floor clean.

---

## Iceberg is evolving *toward* QueryGraph (the tailwinds)

This is why the answer is "yes" rather than "yes, grudgingly." Three pieces of the
spec are converging on exactly what QueryGraph needs:

### 1. REST server-side scan planning is the sanctioned hook

`planTableScan` / `fetchScanTasks` / plan-tasks are **in the Iceberg REST spec** —
the place where a catalog is explicitly allowed to do engine-grade work and return
file-scan tasks. "Bring Sail closer to the data" is not fighting Iceberg; the spec
*invites* it. Codex already serves these endpoints and validates the wire shapes
against `sail_catalog_iceberg::models` (`PlanTableScanRequest`,
`FetchScanTasksResult`, `CompletedPlanningWithIdResult`). This is the single most
important compatibility fact in our favor: the standardized seam for a
Sail-planned catalog already exists, and we already speak it.

### 2. v3 row lineage is a native provenance primitive

Iceberg v3 gives every row a stable `_row_id` and `_last_updated_sequence_number`
that **survive compaction and rewrites**. That is precisely the substrate a
lineage/graph/provenance system wants: "who derived what, from which snapshot,
under which policy" can hang off row lineage instead of being bolted on. Codex's
manifest expansion already tracks `first_row_id` inheritance — so the hook is
half-wired already. The format is moving in QueryGraph's direction.

### 3. Metadata-as-data closes the loop

Iceberg already exposes metadata tables (manifests, files, partitions, snapshots,
history, refs). Sail can serve those as queryable tables, so QueryGraph reasons
over catalog state **in SQL**. And with Grust's `SailGraphStore`, the catalog
graph is *itself* Iceberg tables (`grust_nodes` / `grust_edges`). You never leave
the lakehouse to get the graph — the graph is standard lakehouse data.

---

## The real tension: credential vending vs. governance

This is the one genuinely hard conflict, and it is *not* about format versions —
it is about the read path. The current code punts on it entirely (allow-all,
`Principal::anonymous()`, vends nothing). It must not stay that way.

Current implementation note: LakeCat now resolves typed principals from explicit
principal headers, TypeDID-style agent headers, and bearer authorization headers
before invoking the governance engine. The full governed credential-vending
model described below is still pending.

The moment you vend raw object-store credentials to an external engine for
"drop-in" compatibility, **you lose row/column control** — the engine reads the
files directly. Column masking, row filters, and the QGLake "metadata-visible,
data-denied" broker are *only* enforceable if the read goes **through** a governed
plane, i.e. through Sail planning. The two paths are a spectrum, and LakeCat —
because it owns the policy gate — gets to choose per principal:

```text
                LakeCat policy gate (TypeSec capability + ODRL + TypeDID)
                                     │
        ┌────────────────────────────┴────────────────────────────┐
        │                                                          │
  RAW-CREDENTIAL PATH                                  GOVERNED / SAIL-PLANNED PATH
  (standard, fast, coarse)                             (innovative, fine-grained)
        │                                                          │
  loadCredentials →                                   planTableScan / fetchScanTasks
  vended temp creds                                     │
        │                                              Sail prunes + applies
  engine reads files                                   row filter + column mask
  DIRECTLY from object store                            │
        │                                              returns ONLY the file/columns
  table-grain governance only                          this principal may see
        │                                                          │
  best for: trusted humans,                            best for: agents, untrusted
  bulk ETL on owned data                               principals, cross-tenant,
                                                        the QGLake demo
        │                                                          │
        └──────────────► every access emits audit + OpenLineage + DID ◄──────────┘
```

**The design rule that follows:** the governed path is the **default for agents**;
raw vending is the explicit, audited exception. If QueryGraph's value only exists
on non-standard endpoints, then any plain-Iceberg client using `loadCredentials`
bypasses all of it and the "innovative catalog" degrades into a side-channel. The
policy gate must make ungoverned reads a *deliberate, logged grant*, not the easy
default.

---

## Why "Sail closer to the data" *is* the governance enabler

The payoff of bringing Sail into the catalog process is not primarily speed — it
is that **the masked/filtered plan becomes the only plan that exists.**

Because Sail is Rust-native, it can run in-process with the catalog (Tier 1
below). That lets LakeCat *fuse the policy decision and the physical plan in one
process*: there is no window where an unfiltered plan exists and policy is checked
"on the side." A JVM catalog plus a separate engine cannot do this cheaply — they
exchange table metadata and the engine plans independently, so enforcement is
advisory. Co-locating policy and planning is the real architectural prize, and it
is *only* reachable by bringing Sail to the data. Tier 1 is therefore not an
optimization; it is what makes the governance story true at all.

---

## v3 → v4 posture: anchor on the stable substrate

The honest risk: features that parse *typed manifest internals* become a treadmill
across format bumps. v4 is still moving (adaptive metadata, single-file commits,
manifest changes), so betting typed support on it now is premature. Codex's hedge
— typed parsing for v1–v3, JSON passthrough behind a capability flag for v4 — is
the correct posture, but it means graph/pruning/lineage silently degrade on v4
until typed support lands.

The discipline that resolves it: **build innovation on what survives the version
bump.**

| Version-stable substrate (build here) | Version-specific internals (treat as additive, behind flags) |
| --- | --- |
| Snapshots, snapshot log, refs | Manifest file encoding / layout |
| Schema, field IDs, partition spec, sort order | Deletion vectors vs positional delete files |
| Table/data-file paths, sequence numbers | Adaptive / restructured v4 metadata |
| Row lineage (`_row_id`, last-updated seq) | New physical types as they land |
| The REST protocol (config, load, commit, plan) | Single-file-commit mechanics |

Everything QueryGraph wants — graph, lineage, governance, semantic projections —
can be expressed in terms of the left column. Keep the right column behind
capability flags and converge on typed v4 support once `sail-iceberg` gains it,
rather than maintaining a parallel JSON inspector forever.

---

## Sketch: the Tier‑1 Sail integration, against Sail's real trait

A useful discovery while drafting this: **Tier 1 is partially free today.** Sail
already ships `IcebergRestCatalogProvider`, a `CatalogProvider` *client* that talks
to a remote Iceberg REST catalog. Since LakeCat already serves Iceberg REST, Sail
can plan and scan LakeCat tables **right now** over the standard protocol — no new
code in Sail, just point it at LakeCat:

```text
  Sail session ──► IcebergRestCatalogProvider ──REST──► LakeCat /catalog/v1
                   (exists in sail-catalog-iceberg today)
```

That gets correctness and compatibility immediately. The *deeper* Tier 1 — the
governance/perf prize — is LakeCat implementing `CatalogProvider` **in-process**,
so there is no REST hop and policy+plan fuse:

```text
  Sail session ──► LakeCatCatalogProvider (in-process) ──► LakeCat store + Sail planner
                   no REST hop · policy gate runs inside planning
```

Sail's actual trait (`sail_catalog::provider::CatalogProvider`) already carries the
hooks we need — `create_table`, `commit_table`, `get_table_commits`, plus
namespace/view ops. A LakeCat in-process provider is roughly:

```rust
use async_trait::async_trait;
use sail_catalog::provider::{
    CatalogProvider, CommitTableOptions, CreateTableOptions,
    GetTableCommitsOptions, GetTableCommitsResponse, /* … */ Namespace,
};
use sail_catalog::error::CatalogResult;
use sail_common_datafusion::catalog::{DatabaseStatus, TableStatus};

/// In-process bridge: Sail's planner resolves LakeCat tables without a REST hop,
/// and every privileged op passes the LakeCat policy gate *inside* planning.
pub struct LakeCatCatalogProvider {
    name: String,
    store: Arc<dyn CatalogStore>,          // pointer CAS + management state
    sail: Arc<dyn SailCatalogEngine>,      // commit assembly + scan planning
    governance: Arc<dyn GovernanceEngine>, // mints Capability<Action, Resource>
    principal: Principal,                   // resolved by the auth layer
}

#[async_trait]
impl CatalogProvider for LakeCatCatalogProvider {
    fn get_name(&self) -> &str { &self.name }

    async fn get_table(&self, db: &Namespace, table: &str) -> CatalogResult<TableStatus> {
        // gate(table.load) -> Capability<CanLoad, Table>; the capability is the proof.
        // load the metadata pointer; return Sail's TableStatus.
        todo!()
    }

    async fn commit_table(
        &self,
        db: &Namespace,
        table: &str,
        options: CommitTableOptions, // { format, requirements, updates }
    ) -> CatalogResult<TableStatus> {
        // 1. gate(table.commit) -> Capability<CanCommit, Table>
        // 2. self.sail.prepare_commit(...) validates requirements vs typed metadata
        // 3. write new metadata.json via object_store
        // 4. CAS the pointer in self.store (prev -> new), in one txn with
        //    idempotency + audit + pointer-log + outbox rows
        // 5. return updated TableStatus
        todo!()
    }

    async fn get_table_commits(
        &self,
        db: &Namespace,
        table: &str,
        options: GetTableCommitsOptions,
    ) -> CatalogResult<GetTableCommitsResponse> {
        // serve the metadata_pointer_log as Iceberg-style commit history
        todo!()
    }

    // create_database / create_table / drop_* / views: delegate to store + gate.
    # // (elided)
}
```

Two things this makes concrete:

- **The seam already in Sail (`commit_table` / `get_table_commits` on
  `CatalogProvider`) is exactly the catalog control-plane seam LakeCat needs** —
  the architecture's "extension traits that let an external catalog call Sail for
  planning" is realizable by implementing the trait Sail already defines, not by
  inventing a new one.
- **The policy gate runs *inside* the provider**, so a Sail plan over a LakeCat
  table cannot exist without the capability. That is the fused-policy property that
  raw credential vending can never give you.

Tier 2 (a remote scan-planning entrypoint returning DataFusion-pruned file tasks,
upstreamed into `sail-plan-lakehouse`) then generalizes this to out-of-process
clients, with LakeCat still the policy facade.

---

## Innovative features achievable *within* the constraints

Concretely, all of these conform to Iceberg and exploit Sail-at-the-data:

1. **Policy-aware server-side scan planning** — pruned + column-/row-filtered file
   tasks via the *standard* REST endpoint; agents receive strictly less than
   humans, all within spec.
2. **Provenance on row lineage** — lineage that survives compaction, keyed to v3
   `_row_id`/sequence numbers, emitted as OpenLineage + TypeDID attestations.
3. **The catalog graph as Iceberg tables** — Grust-on-Sail, so graph queries over
   physical + semantic + policy + lineage relationships run on the lakehouse
   engine itself.
4. **Metadata-as-data reasoning** — manifests/stats/deletes as queryable Sail
   tables; QueryGraph asks SQL questions about catalog state.
5. **Semantic projections** (Croissant/CDIF/OSI) as a *derived*, stable-ID-keyed
   layer — already built in `lakecat-querygraph`.
6. **Governed time-travel / incremental** — "what changed between snapshots, and
   who is allowed to see the delta" as a first-class, policy-checked operation
   (Codex already started incremental scan planning).

---

## Division of labor I: LakeCat vs. Grust (the graph)

The architecture says *"Grust owns graph schema, typed/untyped operations,
indexing, and traversals; LakeCat only translates catalog events into graph
mutations,"* and the code already pushes mutations to Grust via
`GrustCatalogGraphSink::emit` → `store.put_graph`. The **direction is right** —
push essentially all graph *mechanics* to Grust. The subtlety is that the current
code pushes the right *category* of work but the wrong *shape* and *granularity*
of data.

**Push to Grust (all the mechanics):**

| Responsibility | Owner | Why |
| --- | --- | --- |
| Persistence (upsert, dedup, `put_graph`) | **Grust** | Its core; already done. |
| Indexing + traversal / pattern / Cypher | **Grust** | Grust owns the traversal IR and `SailGraphStore`/Cypher. |
| Schema enforcement (typed nodes/edges) | **Grust** | `GraphSchemaBuilder` exists; let it validate. |
| Backend choice (memory for tests, `SailGraphStore` for prod) | **Grust** | LakeCat must not know the backend. |
| Graph **algorithms** (PageRank, community, paths) | **A Grust *backend*** (Surreal/Helix/petgraph) | Not Grust-core and *not* LakeCat — Grust is explicit it isn't a petgraph replacement; algorithms run in a backend, orchestrated through Grust. |

**Keep in LakeCat (the irreducible minimum — needs catalog domain knowledge or
transactional coupling, which Grust cannot supply):**

1. **Event → typed-graph translation.** Grust doesn't know what a snapshot,
   manifest, or delete file is, and the mapping is governance-aware (which edges
   encode `CAN_PLAN`/`CAN_READ` for which principal), so it can't be a generic
   Iceberg→graph adapter.
2. **The stable-ID scheme** (`lakecat:table:…`, `lakecat:snapshot:…`). Grust treats
   these as opaque application IDs — by design — but LakeCat mints them.
3. **Transactional consistency** with committed Iceberg state — the outbox. Grust
   persists what it's told; *not lying about uncommitted state* is LakeCat's job.
4. **Projection policy / granularity** — which catalog facts become graph at all.

**The real "how much" question is about data, not logic.** The risk isn't too much
logic in Grust — it's too much catalog state projected into graph form. Naively
materializing every `DataFile`/`DeleteFile`/`Manifest` as a node on every commit
explodes the graph (millions of files per large table, churned on every commit).
The rule: **the graph is for relationships and reasoning; the manifest is for file
enumeration.**

- Project the **stable semantic entities** as nodes — `Project / Warehouse /
  Namespace / Table / View / Column / Snapshot / Policy / Principal / ScanPlan /
  Commit / LineageRun`. Bounded cardinality, high reasoning value.
- Keep **file-granularity out of the graph** — `DataFile / DeleteFile / Manifest`
  live as **metadata-as-data** (queryable Sail/Iceberg metadata tables), joined
  on demand. `SailGraphStore` makes this clean: the graph *and* the manifest
  tables live in the same engine, one query language.

**Two execution fixes** (from OPUS1 Findings 8 and 10): push the *typed taxonomy*
instead of `CatalogEvent` breadcrumbs, and make the write *outbox-driven* instead
of inline `emit().await?` so a graph hiccup never fails a commit and the graph
never reflects a rolled-back one.

---

## Division of labor II: LakeCat vs. QueryGraph (OSI, OpenLineage, TypeSec, Croissant, ODRL)

The question: integrate these standards *into* LakeCat, or make QueryGraph a
plugin/add-on *for* LakeCat? **Neither extreme.** Don't absorb the standards into
the catalog (that bloats it and makes business semantics a catalog concern), and
don't load QueryGraph *inside* the catalog as a plugin (that couples the catalog
to the agent layer). Cut by a single test derived from this document's thesis:

> Is it an **enforcement / production** concern that must run *inside* the
> read/commit path so it can't be bypassed? → **LakeCat primitive** (a trait with
> a pluggable engine). Is it a **description / semantics** concern *derived* from
> committed state for downstream consumers? → **derived layer / QueryGraph**
> (optional, pluggable, bypass-safe).

Applying it, most of these standards **split** across the line:

| Standard | Concern | Owner | In the access path? |
| --- | --- | --- | --- |
| TypeSec capabilities / policy gate | Enforcement | **LakeCat** primitive (`GovernanceEngine`) | **Yes** — unbypassable |
| TypeSec TypeDID / agent delegation / signed summaries | Agent trust mesh | **QueryGraph** | No — above the catalog |
| ODRL — enforceable usage constraints | Enforcement | **LakeCat** (part of the gate) | **Yes** |
| ODRL — published policy documents | Description | Projection / **QueryGraph** | No |
| OpenLineage — catalog ops (commit, scan-plan) | Production | **LakeCat** primitive (`LineageSink`) | **Yes** — it owns those events |
| OpenLineage — cross-run / agent-run aggregation | Aggregation | **QueryGraph** | No |
| Croissant / CDIF | Dataset description | Thin projection in LakeCat OK; rich in **QueryGraph** | No |
| OSI — metrics, dimensions, business semantics | Business semantics | **QueryGraph** | No — a catalog knows columns and types, not "revenue = sum(amount) by region" |

So the answer is **a layered split, not a merge and not a plugin**:

- **LakeCat owns the enforcement/production substrate** — the TypeSec policy gate,
  the *enforceable* subset of ODRL, and *production* of catalog-level OpenLineage —
  as traits with pluggable engines (the shape Codex already has).
- **QueryGraph is the application *above*** LakeCat, owning the genuinely
  application-level concerns: the semantic model (**OSI**), the agent trust mesh
  (**TypeDID** delegation, signed summaries), and cross-run lineage aggregation.
- **QueryGraph plugs *into* LakeCat's seams** (`GovernanceEngine`, `LineageSink`,
  `CatalogGraphSink`, a projection hook) and **consumes the governed read path** —
  it does not get loaded inside the catalog.

**Crucially, the governance requirement does *not* force in-process coupling.**
Agents get full row/column governance simply by reading through LakeCat's governed
scan-planning endpoint; QueryGraph can therefore be a separate service and still
be fully governed. "Add-on above" beats "plugin inside."

**Dependencies point one way: QueryGraph → LakeCat → Sail.** LakeCat must never
import QueryGraph or hardcode OSI/Croissant semantics. This is also a refinement of
the current `lakecat-querygraph` crate: a *thin* Croissant/CDIF projection is a
defensible catalog discovery feature ("describe my datasets"), but the **OSI
projection over-reaches** — the authoritative semantic model belongs in QueryGraph.
Keep the thin bootstrap in LakeCat; move the rich semantic model up.

---

## Anti-patterns that *would* break compatibility

Name them so they stay out:

- Putting business/policy semantics **only** in Iceberg metadata.
- Requiring **non-standard endpoints** for normal table reads.
- **Forking** the format or adding mandatory custom metadata that standard engines
  choke on.
- Letting the graph **drift** from committed snapshots (eventual-consistency lies).
- Defaulting agents to **raw credential vending** (ungovernable reads).

---

## Verdict

The design is sound and well-timed: Iceberg's trajectory (REST scan planning, row
lineage, metadata tables, *optional* credential vending) is converging on what
QueryGraph needs. The goals reinforce each other if three disciplines hold:

1. **Innovation lives in the control plane and derived/engine layers** — never in
   mandatory Iceberg metadata.
2. **The governed (Sail-planned) path is the default for agents**; raw credential
   vending is the audited exception.
3. **Anchor on the version-stable substrate**; treat typed v4 as additive, behind
   flags.
4. **Dependencies point inward** — QueryGraph → LakeCat → Sail, never the reverse;
   LakeCat never imports QueryGraph, owns graph algorithms, or hardcodes a
   business-semantic standard. Enforcement/production are LakeCat primitives;
   description/semantics are derived layers above it.

The thing standing between this and a real demonstration is not a compatibility
conflict — it is that today's code is all Tier 0 (Sail as a struct library) with
no auth, no real commit, and no governed read path. Implement the auth +
persistence + real-commit spine from OPUS1, promote Sail to Tier 1 (start free via
`IcebergRestCatalogProvider`, then the in-process `LakeCatCatalogProvider`), and
drive the QGLake "Resilience Desk" demo end-to-end. That demo passing *is* the
proof that innovation and Iceberg conformance coexist.

---

## Review log & working plan (dev-manager view)

Working mode from here: **Codex implements; this reviewer prioritizes, reviews each
slice, and keeps the plan honest.** This section is the living tail of the doc —
append a dated entry per slice; keep the finding-status table current.

### Review gate (applies to every slice)

1. **Verify on the default feature set, not only `--all-features`.** `--all-features`
   turns `sail-local` on and can *mask* a broken default build. A slice is not
   "green" until `cargo test --workspace` (default) passes too. "Expected to pass"
   is not "verified."
2. **Don't deepen a layer that already works while a load-bearing layer is
   missing.** Tier-0 (in-process pruning with Sail structs) is the most-developed
   part of the system; further depth there has diminishing returns until the
   governed read path exists to make it end-to-end.
3. **Each privileged path must be unbypassable** (capability in the signature, not
   a boolean checked on the side) and **emit audit + lineage**.

### Verification snapshot — 2026-06-16

| Check | Result |
| --- | --- |
| `cargo test --workspace` (default) | **GREEN** |
| `cargo test --workspace --all-features` | **GREEN** (incl. `lakecat-sail` 10, `lakecat-service` 8, Turso store test) |
| `cargo test -p lakecat-store --features turso-local` | **GREEN** — Turso namespaces/tables/idempotent commit/pointer-log/audit/outbox |
| `cargo test -p lakecat-service --features turso-local` | **GREEN** — binary compiles and tests with optional Turso store path |
| New `preserves_filter_context_and_prunes_loaded_file_bounds` | **GREEN** — bounds survive Avro round-trip *and* prune end-to-end (2 files in, `eq` filter, 1 out) |
| Structured plan tokens now **table-bound** | confirmed (`"does not match requested table"`) |

### Finding status (from [OPUS1.md](OPUS1.md))

| # | Finding | Status | Evidence / note |
| --- | --- | --- | --- |
| 1 | Red default-feature tests | **CLOSED** | default workspace tests pass; Sail-specific service assertions are gated |
| 2 | No auth / real principal | **PARTIAL** | principal/TypeDID/bearer header resolution added with sanitized `lakecat.request-identity.v1` envelopes on authorization receipts; catalog config, namespace create/list, table create/load/commit, scan planning, task materialization, credential-vending requests, and QueryGraph bootstrap now require typed capabilities; catalog/namespace/table/scan/bootstrap/credential events and commit receipts persist in Turso audit/outbox; storage-profile modeling, a pluggable credential issuer, and a TypeSec-gated `typesec://` issuer are started, but real TypeDID verification and external secret-store resolver backends are still pending |
| 3 | No durable / CAS commit | **CLOSED for local durable spine** | Turso commits now write local `file://` metadata when provided by the Sail-facing plan, advance pointers with expected-previous compare-and-swap, persist idempotency/audit/outbox rows, and have a concurrent writer regression |
| 4 | No persistence backend | **PARTIAL** | Turso `TursoCatalogStore` exists for namespaces, tables, pointer log, idempotency, audit, outbox, object-write-aware commits, outbox delivery, typed inferred storage profiles, governed managed warehouse storage profiles with external secret references, a service credential-issuer hook, a TypeSec-gated `typesec://` issuer path, governed ODRL policy bindings, and governed table soft delete/restore; external secret-store resolver backends remain pending |
| 5 | Service can't activate real engines | **CLOSED** | `sail-local` / `typesec-local` / `grust-local` passthroughs now in `lakecat-service` |
| 6 | Sail used as struct library, not planner | **STARTED** | `lakecat-sail/catalog-provider` now exposes a governed in-process Sail `CatalogProvider` over LakeCat namespaces/tables/commits, pointer-log-backed commit discovery, and basic Iceberg current-schema/nested-type/partition/sort-order/identifier-field `TableStatus`; remaining-constraint conversion and deeper planner fusion remain pending |
| 7 | Plan ↔ implementation drift | **PARTIAL** | architecture and OPUS working-plan docs now track the committed Turso CAS/object-write/outbox/OpenLineage/storage-profile and in-process Sail provider slices; remaining drift risk is around Grust taxonomy placement and remote credential issuance |
| 8 | Grust graph is a placeholder taxonomy | **OPEN** | |
| 9 | `list_namespaces` fabricates `default` | **CLOSED** | memory and Turso stores return an empty list until namespace creation |
| 10 | Side effects coupled to request path | **CLOSED** | Turso outbox and service drain/projection API exist; catalog handlers record durable events and no longer emit graph/lineage inline |
| 11 | Plan tokens unauthenticated / leak paths | **CLOSED** | new structured Sail plan-task tokens are table-bound, path re-validated, and HMAC-signed; legacy unsigned structured tokens remain decodable for compatibility |
| 12 | v4 = JSON passthrough, thin coverage | **OPEN (by design)** | keep behind capability flag |

### Latest slice reviewed — scan-filter / file-bound pruning

**Verdict: accept (sound, and better than described), with two corrections.** The
structured filter-context tokens, conservative pruning (missing-metrics-keeps-file),
and the end-to-end Avro-round-trip test are correct work, and the token
**table-binding** materially hardens Finding 11. Corrections: (a) the default build
is **still red** — gate the two tests and add a default-feature CI row; (b) the
"make manifest-load preserve metrics" framing is a misdiagnosis — the load path
already preserves bounds (the new test proves it); the real gap is surfacing
**Parquet column statistics at write/commit time**, plus the service fixture simply
writing bounds.

### Latest implementation slice — Turso persistence spine

**Status: implemented for the local durable spine, partial P1 overall.** Added
`lakecat-store/turso-local` with a Turso `TursoCatalogStore` for namespaces, table
records, metadata pointer log, idempotency records, audit events, and outbox
events. The service binary can use it when built with `turso-local` and
`LAKECAT_TURSO_PATH` is set. Since the first Turso slice, LakeCat now writes local
`file://` metadata objects when the Sail-facing commit plan carries new metadata,
advances table pointers with expected-previous compare-and-swap, persists
authorization/audit/outbox receipts across privileged catalog paths, and drains
committed outbox events into graph and lineage sinks. LakeCat also has a typed
inferred storage-profile model for conservative credential responses: local
`file://` tables can return scoped no-secret profile hints, while remote
object-store locations return no credentials until short-lived issuance exists.
Governed management endpoints now upsert/list warehouse storage profiles and the
Turso store persists them for longest-prefix credential selection. LakeCat also
persists governed ODRL policy bindings and attaches active table bindings to the
authorization context before TypeSec runs. Governed table lifecycle now hides
soft-deleted tables from normal catalog reads, restores them through a governed
management endpoint, and emits `table.deleted` / `table.restored` audit/outbox
events. The remaining P1/P2 work is remote object-store credential issuance,
external secret-store integration, and broader operator APIs.

### Reviewer note — endorse the Turso pivot, with a gate (2026-06-16)

**Endorsed.** Replacing C-SQLite with **Turso** (pure-Rust, SQLite-compatible) is
the right call and on-thesis: it removes the last non-Rust dependency from the
embedded path, is async-native (no blocking thread pool), and — because Turso
speaks SQLite — kept the schema, migrations, and store as a contained driver swap
behind `CatalogStore`. Verified locally: `TursoCatalogStore` uses a real
`conn.transaction()` / `tx.commit()` scope, and the `turso-local` store and service
tests plus the default workspace are green.

**Gate before P1 leans on it (Finding 3 stays OPEN):** the Turso Rust engine is
young (pre-1.0). Two things to verify on the pinned version before the durable
commit hangs on it:

1. **Real CAS, not just a transaction.** The store has the transaction scaffold but
   now compares against the expected previous pointer and updates through
   `UPDATE tables SET … WHERE table_key = ? AND metadata_location = :prev`;
   `rows_affected == 0` is a concurrency conflict. The service can now advance
   the pointer from a Sail-facing LakeCat `metadata-location` commit extension;
   local `file://` object metadata writes are implemented for commit plans that
   carry new metadata.
2. **Isolation actually holds** under concurrent commits to the same table. A
   Turso regression now races two writers against the same expected metadata
   pointer and verifies one commit succeeds while one returns a conflict.

Keep the schema portable, but treat the Rust `turso` crate as the selected path
for LakeCat's durable local spine. A future Postgres/SQLx or libSQL-client backend
should be a deliberate product/runtime choice, not the default next step; the
`CatalogStore` seam makes that possible without changing catalog semantics.

### Priority-ordered plan (what to build next)

Reconciles the OPUS1 milestones with the push-back above. Resume Tier-0 pruning
only once **P2** gives it a governed path to run on.

- **P0 — make `cargo test --workspace` green.** Done locally, with a CI matrix
  added over `{default, sail-local, typesec-local, grust-local, turso-local,
  all-features}` and sibling Sail/Grust/TypeSec checkouts matching LakeCat path
  dependencies. (OPUS1 Milestone 0; Finding 1.) *Smallest unblock.*
- **P1 — persistence + durable commit spine.** `TursoCatalogStore`;
  Sail-assembled metadata → `object_store` write → pointer CAS → idempotency/audit/
  pointer-log/outbox in one txn. (Milestones 2–3; Findings 3, 4, 10.) *Turso
  store plus pointer-log/idempotency/audit/outbox rows, CAS semantics, local
  object metadata writes, a typed store-level outbox drain API, and service-level
  graph/lineage projection are now implemented for the local durable spine;
  catalog handlers rely on durable outbox delivery instead of inline
  graph/lineage side effects. Typed inferred storage profiles, governed
  storage-profile management endpoints, external secret references, a pluggable
  credential issuer, a TypeSec-gated `typesec://` issuer path, and Turso-backed
  longest-prefix profile selection are started. Governed ODRL policy-binding
  management is started and active table bindings flow into authorization
  context. Governed table soft delete/restore is started; real external
  secret-store resolver backends remain outstanding.*
- **P2 — finish governance: capability model + governed read path.** Promote the
  boolean receipt to `Capability<Action, Resource>`; route agent reads through
  governed scan-planning; persist the receipt with the audit row. (Milestones 1, 5;
  Finding 2.) *Started with typed catalog-config, namespace-create,
  namespace-list, table-create, table-load, table-scan, table-commit,
  credential-vend, and graph-read capabilities, persisted commit authorization
  receipts, and durable catalog-config / namespace-create / namespace-list /
  table-create / metadata-read / scan-planning / scan-task-fetch /
  credential-vending / QueryGraph-bootstrap audit/outbox records. Authorization
  receipts now carry a sanitized request-identity envelope for TypeDID/agent,
  bearer-token, delegation, and signed-summary material; real TypeDID signature
  verification remains pending in the TypeSec integration.*
- **P3 — Sail Tier 1 (`CatalogProvider`).** Start "free" via Sail's
  `IcebergRestCatalogProvider` over REST, then the in-process `LakeCatCatalogProvider`
  so policy + plan fuse. (Milestone 6; Finding 6.) *Started with a feature-gated
  `lakecat-sail/catalog-provider` bridge implementing Sail's `CatalogProvider`
  over governed LakeCat namespace/table create/load/list/drop/commit paths, with
  `get_table_commits` served from the metadata pointer log and basic Iceberg
  current-schema, nested types, partition fields, sort order, and identifier
  fields projected into Sail `TableStatus`. The next step is full Iceberg
  metadata-to-`TableStatus` conversion for remaining constraint forms and planner
  fusion, preferably upstreamed into Sail helpers where reusable.*
- **P4 — typed Grust catalog graph + outbox**, then **OpenLineage + TypeDID**.
  (Milestones 7–8; Findings 8, 10.) *Started for catalog-level OpenLineage:
  outbox-drained namespace/table lineage events now project to OpenLineage-shaped
  payloads and hash receipts in `lakecat-lineage`; TypeDID request envelopes are
  captured on receipts, but verification/attestation and typed Grust graph
  taxonomy remain pending.*
- **P5 — QueryGraph end-to-end.** `querygraph import-lakecat`; QGLake "Resilience
  Desk" as the acceptance test. (Milestone 9.)
- **Deferred — Tier-0 pruning depth and typed v4.** Good but diminishing-returns
  until P1–P3 land; HMAC-signed plan-task tokens are now implemented for new
  Sail scan-planning tokens. (Milestone 10; Finding 12.)
