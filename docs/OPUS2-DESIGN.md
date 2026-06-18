# LakeCat Design Plan (OPUS2-DESIGN)

Design-level companion to [OPUS2.md](OPUS2.md), succeeding
[OPUS1-DESIGN.md](OPUS1-DESIGN.md). OPUS1-DESIGN asked *"can we get innovative
QueryGraph features while conforming to Iceberg and bringing Sail to the data?"*
and answered **yes**, then sketched the auth + persistence + commit spine and the
Tier-1 Sail bridge. That spine is now built (see OPUS2). This document updates the
thesis to the current reality, names the one discipline now on the critical path,
and re-prioritizes the plan.

The 2026-06-16 verdict still holds — the goals reinforce rather than conflict —
but the binding constraint has moved. It is no longer *"there is no governed
plane."* It is *"the governed plane gates but does not yet narrow."*

---

## Thesis (unchanged, now load-bearing)

Iceberg constrains one layer: how table state is stored and how engines talk to
you. Everything QueryGraph wants — graph, lineage, governance, semantic
projections — lives in the unconstrained control-plane and derived layers. Keep
Iceberg pristine at the boundary; innovate inside.

```text
┌─────────────────────────────────────────────────────────────────────┐
│  DERIVED / SEMANTIC   (projection of committed state, never a 2nd truth) │
│    Grust catalog graph · Croissant/CDIF/OSI · ODRL docs · OpenLineage │
│  ┌───────────────────────────────────────────────────────────────┐   │
│  │  CONTROL PLANE   ✅ built: identity · Capability<A,R> · pointer │   │
│  │     CAS · idempotency(store) · audit · outbox                  │   │
│  │     ✅ built: ODRL→restriction · governed plan/fetch proof       │   │
│  │  ┌─────────────────────────────────────────────────────────┐ │   │
│  │  │  ENGINE (Sail)  ✅ Tier-1 provider for commits           │ │   │
│  │  │     ⛔ more read execution belongs upstream in Sail      │ │   │
│  │  │  ┌───────────────────────────────────────────────────┐  │ │   │
│  │  │  │  ICEBERG FLOOR ✅ pristine: metadata/manifests/REST│  │ │   │
│  │  │  └───────────────────────────────────────────────────┘  │ │   │
│  │  └─────────────────────────────────────────────────────────┘ │   │
│  └───────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

The ✅ rows are what OPUS1-DESIGN planned and the repo delivered. The ⛔ rows are
the whole of the remaining critical path.

---

## The critical path is now one idea: the restriction

OPUS1-DESIGN framed the hard problem as "credential vending vs. governance" and
drew a two-path diagram (raw creds vs. Sail-planned). That framing was right, and
the gate that chooses between paths now exists. What does **not** exist is the
object that makes the governed path mean anything:

> **The restriction**: the server-derived, principal-specific set of *allowed
> columns* and *row predicate* (plus purpose and credential TTL) that a read is
> narrowed to — derived from policy, never from client input, carried by the
> capability, applied by the planner, and recorded in the receipt.

Everything else on the frontier is in service of this one object:

- **ODRL bindings → restriction** (OPUS2 F2). The enforceable subset of ODRL
  (allowed columns, row predicate, purpose, max TTL) is *parsed into* a
  restriction. `Delegate` consults RBAC instead of failing closed. Bindings stop
  being opaque context JSON.
- **Restriction → Sail-planned read** (OPUS2 F1, F5). The restriction becomes the
  plan's mandatory projection/filter intersection, applied *through* the
  in-process provider so policy and plan fuse in one process — the property
  OPUS1-DESIGN called the architectural prize, now with something to enforce.
- **Restriction → capability → receipt.** The capability carries the *effective*
  columns/predicate; the audit receipt records what was enforced, not just that
  access was allowed; `fetch-scan-tasks` re-applies the same restriction so a
  token cannot widen a projection.

When the restriction exists end to end, the QGLake "metadata-visible,
data-denied" broker is a configuration of it (allowed-columns = ∅, metadata =
visible), not new machinery. That demo passing is still the proof that
innovation and Iceberg conformance coexist.

---

## Why this is reachable now (the tailwinds still hold)

The three Iceberg tailwinds OPUS1-DESIGN identified are unchanged and now have
landed substrate to attach to:

1. **REST server-side scan planning is the sanctioned hook** — and LakeCat now
   serves `plan`/`fetch-scan-tasks` with table-bound, HMAC-signed, path-revalidated
   plan-task tokens. The restriction rides the same tokens.
2. **v3 row lineage** — the commit path already records `metadata_pointer_log`
   with principal and request hash; OpenLineage/DID provenance hangs off it.
3. **Metadata-as-data** — the cardinality discipline (semantic entities in the
   graph, files as metadata-as-data) is now enforceable because the graph
   projection is Grust-owned and the manifest path stays in Sail.

The in-process `LakeCatCatalogProvider` is the lever: it already fuses governance
for commits. Extending the same fusion to reads is the mechanical step that turns
the gate into a filter.

---

## Division of labor (reaffirmed against the current code)

The boundaries OPUS1-DESIGN drew held under 2.5× growth — keep them:

| Concern | Owner | Status |
| --- | --- | --- |
| Graph schema, taxonomy, traversal, Cypher, stores | **Grust** | ✅ taxonomy + ingestion + Cypher boundary moved into Grust |
| Iceberg models, status conversion, planning helpers, manifest IO | **Sail** | ✅ conversion + planning helpers exported; ⚠️ commits blocked from upstream push (OPUS2 F10) |
| TypeDID envelopes, attestation, agent trust mesh, signed summaries | **TypeSec / QueryGraph** | ✅ attestation API in TypeSec; verifier seam in LakeCat |
| Identity, capability gate, pointer CAS, audit, outbox, **the restriction** | **LakeCat** | ✅ gate + enforced restriction proof |
| Event → typed-graph mapping (catalog-domain) | **LakeCat (thin)** | ⚠️ still breadcrumbs (OPUS2 F6) |
| OSI semantic model, cross-run lineage aggregation | **QueryGraph** | n/a (above LakeCat) |

Dependencies still point one way: **QueryGraph → LakeCat → Sail**. The one
process risk is F10 — LakeCat depends on local, un-pushed Sail commits, so "push
reusable work upstream, then depend on it" is only half-true today. Resolve by
landing those Sail commits upstream (or pinning a published Sail) so the green
build is reproducible off this machine.

---

## Anti-patterns to keep out (unchanged, plus one)

- Business/policy semantics **only** in Iceberg metadata.
- Non-standard endpoints for **normal** table reads.
- Forking the format / mandatory custom metadata standard engines choke on.
- Graph **drift** from committed snapshots.
- Defaulting agents to **raw credential vending**.
- **New:** letting the **client** supply the projection/filter that governance is
  supposed to enforce. The restriction is server-owned; client projection may
  only ever *narrow within* it, never define it.

---

## Finding status (from [OPUS2.md](OPUS2.md))

| # | Finding | Severity | Status |
| --- | --- | --- | --- |
| F1 | Governed read gates but does not mask | HIGH | STARTED — fine-grained restrictions now block raw credential bypass |
| F2 | ODRL transported, not interpreted; `Delegate` → deny | HIGH | STARTED — shared restriction parser plus TypeSec RBAC policy loading |
| F3 | Commit idempotency unreachable from REST | MEDIUM | STARTED — REST header replay + mismatch guard wired |
| F4 | Metadata written before CAS; no orphan handling/retry | MEDIUM | STARTED — local orphan cleanup |
| F5 | Scans bypass the in-process provider | MEDIUM | STARTED — REST `sail-local` plan and fetch routes now use provider seams |
| F6 | Catalog graph is event breadcrumbs | MEDIUM | STARTED — bounded taxonomy replays through Grust |
| F7 | Tenancy hierarchy durable but not fully routed | LOW | STARTED — Server/Project/Warehouse/View records and registered warehouse-prefixed routing |
| F8 | Production secret backends unexercised | LOW | STARTED — all accepted production secret-ref schemes are TypeSec-gated before fail-closed resolver errors |
| F9 | v4 JSON passthrough | LOW | OPEN by design |
| F10 | Sibling deps local-only; CI manual | LOW (process) | OPEN |

---

## Priority-ordered plan (what to build next)

The persistence/commit/auth spine (old P0–P3) is done. Re-baselined from here:

- **P1 — The restriction, end to end (F2 → F1 → F5).** The defining slice.
  1. Parse the enforceable ODRL subset of a `PolicyBinding` into a typed
     `ReadRestriction { columns, row_predicate, purpose, max_ttl }`; compose
     `Delegate` onto RBAC instead of denying. *Started for allowed-columns and
     purpose extraction from enforced policy bindings; row-predicate and max
     credential TTL extraction now support nested LakeCat restriction fields and
     ODRL constraints; TTL composition chooses the shortest governed lifetime
     and malformed TTLs fail closed; the parser/composer and
     restriction-application helpers now live in `lakecat-security` so the REST
     route and provider scan path share one governance primitive; TypeSec
     delegate-to-fallback composition is wired at the LakeCat governance wrapper
     seam, and the service binary can now load a TypeSec RBAC YAML fallback
     policy through `LAKECAT_TYPESEC_RBAC_POLICY`.*
  2. Carry the effective restriction in `TableScanCapability` and record it in
     the audit receipt (`policy_hash` includes the binding's ODRL hash).
     *Started: scan and credential-vend receipts now carry `read-restriction`
     with allowed columns and policy hashes, and LakeCat stamps the receipt's
     top-level `policy_hash` from those enforced ODRL hashes while preserving
     the governance engine hash as an input; credential-vend capabilities expose
     the same restriction and LakeCat now withholds raw credentials when row or
     column restrictions require a governed Sail-planned read; `table.scan-planned`
     audit/outbox payloads surface the effective restriction plus storage/metadata
     locations for OpenLineage and graph consumers; `table.scan-tasks-fetched` now
     surfaces the same governed context, routes through the scan projection
     sink path, and the Iceberg REST `fetchScanTasks` response now carries a
     `lakecat:fetch-scan-tasks` extension with the re-applied restriction;
     governed credential-vend receipts are marked as raw-credential exceptions,
     and the `credentials.vend-attempted` audit/outbox payload surfaces the same
     restriction and exception marker at top level so consumers can distinguish
     raw credential exceptions from the preferred Sail-planned read path.*
  3. Apply it as a mandatory projection/filter through a Sail-planned read that
     flows through `LakeCatCatalogProvider`; re-apply on `fetch-scan-tasks`.
     *Started: scan planning intersects client projection with allowed columns,
     appends policy-derived row predicates before calling Sail, and
     revalidates fetch-scan-tasks tokens against the current server-derived
     restriction through shared `ReadRestriction` methods; the in-process
     provider can now mint scan capabilities with stored policy-binding context,
     plan governed scans by applying restriction projection/filters before
     calling Sail, and fetch scan tasks by re-applying mandatory projection and
     filter requirements before Sail expands plan-task tokens; the REST
     `sail-local` planning and fetch endpoints now route through these provider
     seams.*
  *Smallest end-to-end version landed first: a single allowed-columns list is
  enforced on one table, proven by a test where an agent asks for two columns
  and Sail receives one.*
- **P2 — QGLake "Resilience Desk" acceptance (depends on P1).** Wire
  `querygraph import-lakecat` and run the broker demo: supervisor delegates to
  specialists that plan scans *through LakeCat*, each gated + restricted, each
  recorded via OpenLineage + DID, synthesis over signed summaries only. This is
  the GOAL's acceptance target; P1 is the only thing it's missing. *Started:
  `lakecat-cli qglake-fixture` now creates a table with a restricted
  `raw_payload` column and installs a parser-verified
  `lakecat:read-restriction` so the live fixture exercises projection
  narrowing, row predicates, and credential TTL before QueryGraph import; the
  fixture now performs a live scan-plan verification that asks for
  `raw_payload` and fails unless LakeCat narrows the effective projection and
  preserves the row predicate and policy hash; the fixture now also requires a
  plan-task token, posts that token to `fetchScanTasks`, and fails unless the
  fetch response carries the re-applied restriction with the same policy hash
  proof; the fixture now writes fetchable local Iceberg manifest metadata for
  the bootstrap table so that acceptance exercises real plan-task expansion
  instead of a schema-only table; the fixture now also probes `loadCredentials` for the
  restricted table and fails unless LakeCat withholds raw credentials from
  agents while still returning an audited standard credential response to a
  trusted human principal for the same table; rerunning the
  fixture now accepts existing namespace/table resources only after validating
  that they still match the expected QGLake fixture shape; the fixture now
  verifies the exported QueryGraph bootstrap contains the enforced QGLake policy
  binding, restricted ODRL material, OpenLineage output, and the fixture table's
  graph node plus namespace edge before writing the bundle; the bootstrap manifest now hashes
  the catalog graph so QueryGraph import can reject graph projection drift;
  `querygraph.bootstrap` outbox events now project into LakeCat OpenLineage
  output events carrying the bootstrap authorization/request-identity payload
  plus verified bundle, graph, OpenLineage, and QueryGraph import-compatibility
  hashes for QueryGraph replay;
  LakeCat now exposes a governed lineage-drain
  management endpoint and `lakecat-cli qglake-fixture` drains the outbox after
  writing the verified bootstrap bundle, failing the fixture if the drain
  delivered zero events and requiring the drain receipt to show
  `querygraph.bootstrap` lineage replay; the drain response now reports
  delivered event types plus graph and lineage projection counts; the embedded
  memory store now records audit events into the same lineage-and-graph outbox
  envelope so default local acceptance runs exercise real replay rather than a
  no-op drain;
  `/querygraph/v1/bootstrap` now exports the stored table policy bindings in
  each table projection and hashes them in the manifest, so QueryGraph imports
  the actual LakeCat policy documents used for governed planning; bootstrap
  manifests now also carry a `querygraph-import` compatibility contract with a
  table-only bundle hash matching the current QueryGraph Rust importer hash
  domain, and QGLake acceptance refuses bundles that drop that import evidence;
  `querygraph.bootstrap` audit/outbox replay now also carries that import hash
  into lineage-drain summaries and the OpenLineage bootstrap facet so acceptance
  proves the durable replay stream matches the same QueryGraph import contract.*
- **P3 — Commit hardening (F3, F4).** Wire REST idempotency keys into the
  existing store replay; make metadata writes survive CAS conflict (finalize
  after win, or bounded re-plan + orphan cleanup); generalize the writer beyond
  `file://` to the declared `object_store` backends. *Started: REST commits now
  accept validated `x-lakecat-idempotency-key` values and replay through the
  store idempotency record instead of creating a second pointer-log row; reused
  keys with different request hashes now return conflict; failed pointer commits
  now clean up newly written local metadata objects when they do not become the
  table's metadata pointer; metadata object writes and cleanup now route through
  `object_store::parse_url_opts`, preserving local `file://` behavior while
  moving the writer toward configured object-store backends.*
- **P4 — Semantic catalog graph (F6).** Emit the bounded typed taxonomy
  (Namespace/Table/Column/Snapshot/Policy/Principal/ScanPlan/Commit) through the
  outbox into Grust; keep file-granularity as metadata-as-data. Then OpenLineage
  transport + TypeDID attestation on the same events. *Started:
  `namespace.created` outbox replay now emits a catalog-facing `Namespace` graph
  event with a stable namespace subject and the same authorization payload used
  for lineage; `policy-binding.upserted` replay now emits a catalog-facing
  `Policy` graph event with stable policy subject, ODRL material, and
  authorization payload; `table.scan-planned` and `table.scan-tasks-fetched`
  replay now emit stable catalog-facing `ScanPlan` graph events with governed
  read restrictions; `table.commit` replay now emits stable catalog-facing
  `Commit` graph events keyed by table and committed sequence; non-anonymous
  resolved principals now replay as stable catalog-facing `Principal` graph
  events; table metadata graph summaries now replay as stable catalog-facing
  `Column` and `Snapshot` graph events; table events continue through the
  Grust-owned event graph adapter.*
- **P5 — Tenancy (F7) + production credentials (F8).** Project/Warehouse as
  stored entities with management endpoints; real Vault/AWS/GCP/Azure resolvers
  behind the TypeSec gate. Needed for multi-tenant deployment, not for the demo.
  *Started: governed server, project, and warehouse list/upsert management
  endpoints now persist durable `ServerRecord`, `ProjectRecord`, and
  `WarehouseRecord` values in memory and Turso stores; `ProjectRecord` can now
  attach to a stored `ServerRecord`, stores reject project attachments to missing
  servers, and stores reject warehouse attachments to missing projects;
  `project.upserted` / `warehouse.upserted` outbox replay emits catalog-facing
  Project and Warehouse graph anchors; server writes emit audited `server.*`
  events while leaving reusable graph hierarchy work to Grust; management routes
  now use the requested warehouse instead of the configured default, Iceberg REST
  routes now accept a warehouse prefix only after resolving a durable
  `WarehouseRecord`, and project-scoped management routes can list/upsert
  warehouses under their durable project while keeping the unprefixed
  default-warehouse compatibility path; credential-vend attempts with
  fine-grained restrictions now fail closed into governed Sail-planned reads
  before any secret resolver is called; all accepted production secret-ref
  schemes are now exercised through TypeSec authorization before failing closed
  when no resolver backend is configured; governed view list/upsert/drop
  management endpoints now persist and delete durable `ViewRecord` values in
  memory and Turso and emit audited `view.*` events; `ViewRecord` now carries
  durable typed columns, warehouse-prefixed catalog REST aliases now list, load,
  upsert, and drop those durable views, and the in-process Sail provider can
  create/load/list/drop them as `TableKind::View` statuses; governed namespace
  load/drop is now wired on unprefixed and warehouse-prefixed Iceberg REST paths
  and through the in-process Sail `CatalogProvider` `drop_database` path, with
  non-empty guards across tables, views, and policy bindings plus audited
  `namespace.dropped` graph/lineage projection on the REST path; QueryGraph
  bootstrap now exports those stored views with manifest-covered OSI hashes,
  typed view columns, view-aware graph edges, and OpenLineage view counts. Full
  Iceberg view version/commit semantics remain pending.*
- **P6 — Reproducibility (F10) + typed v4 (F9).** Land the Sail helper commits
  upstream (or pin a published Sail) and re-enable automatic CI; converge on
  typed v4 metadata once `sail-iceberg` provides it.

---

## Verdict

LakeCat crossed the line OPUS1 drew: it is a real, governance-gated,
durably-committing Iceberg catalog with the seams pointed the right way and the
boundaries intact. The design bet is being vindicated by the code. The
restriction is now parsed from ODRL, carried into governed scan/credential
receipts, re-applied through plan-task fetch, and used to steer agents onto the
governed Sail-planned path while trusted humans keep audited standard credential
vending. The remaining distance to the GOAL is short and specific — push more
read execution upstream into Sail, keep QGLake's acceptance fixture on real
manifest-backed metadata, and continue tightening the byte-compatible proof for
standard Iceberg clients.
