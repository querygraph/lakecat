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
| F9 | v4 JSON passthrough | LOW | OPEN by design; stronger bridge fixtures now cover JSON summary inspection, manifest-list planning, stateless fetch-token validation, and stable commit requirements |
| F10 | Sibling deps local-only; CI manual | LOW (process) | OPEN; executable dependency-contract audit now guards local/manual runs |

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
     lineage-drain summaries now expose compact scan-plan, file-scan,
     delete-file, and child-plan task counts, and QGLake saved replay rejects a
     drain that does not prove both scan planning and scan-task fetch replay;
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
  the bootstrap table, including a position-delete manifest, so that acceptance
  exercises real plan-task expansion and Sail delete-file references instead of
  a schema-only table; the fixture now also probes `loadCredentials` for the
  restricted table and fails unless LakeCat withholds raw credentials from
  agents while still returning an audited standard credential response to a
  trusted human principal for the same table, and lineage-drain acceptance now
  verifies both credential-vend audit events survive outbox replay with lineage
  sink receipt hashes plus the trusted-human raw credential exception reason;
  saved replay verification prints compact credential replay evidence for the
  blocked restricted-agent path and the audited trusted-human exception path;
  rerunning the fixture now accepts existing
  namespace/table resources only after validating
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
  delivered event types plus graph and lineage projection counts, and saved
  replay verification prints compact management replay counts for the durable
  tenant spine, policy-list read, and storage-profile read; the embedded
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
  proves the durable replay stream matches the same QueryGraph import contract;
  `scripts/qglake-handoff-local.sh` now turns the saved-artifact workflow into a
  local acceptance harness by starting LakeCat, generating paired bootstrap and
  drain artifacts, verifying replay with LakeCat, and running QueryGraph's
  `lakecat-verify` and `lakecat-import` over the same bundle without mutating
  the QueryGraph checkout; the harness now writes `handoff-summary.json` plus
  captured LakeCat replay and QueryGraph verify/import outputs so operators and
  automation can consume the accepted artifact set without scraping stdout; the
  summary now embeds QueryGraph-verified table/view counts and semantic
  bundle/graph/OpenLineage/import hashes plus standards beside raw artifact
  file hashes, and the harness fails closed unless LakeCat replay, QueryGraph
  verify, and QueryGraph import agree on those semantic counts, hashes, and
  standards; LakeCat replay JSON and the summary now also carry structured
  scan, management, credential, and table-commit replay evidence for
  automation, with explicit schema versions for replay verification JSON and
  the handoff summary.*
- **P3 — Commit hardening (F3, F4).** Wire REST idempotency keys into the
  existing store replay; make metadata writes survive CAS conflict (finalize
  after win, or bounded re-plan + orphan cleanup); generalize the writer beyond
  `file://` to the declared `object_store` backends. *Started: REST commits now
  accept validated `x-lakecat-idempotency-key` values and replay through the
  store idempotency record before Sail validation or metadata-object writes,
  so exact retries are side-effect-free and can still replay after the table has
  advanced; table commit records now carry `format_version`, `snapshot_id`, and
  `policy_hash` summary evidence plus a durable `response_hash` beside the
  request hash so pointer-log, audit/outbox, graph, and lineage replay can prove
  both the Iceberg/governance summary and the exact stored commit response;
  service coverage now proves exact idempotent retries return before
  metadata-object writes by preserving the committed object unchanged on replay;
  a governed management read now exposes compact table commit records and
  records `table.commits-listed` outbox/OpenLineage evidence for QueryGraph and
  operators; QGLake acceptance now performs an idempotent commit-history probe,
  verifies the compact pointer-log evidence including Iceberg format-version
  and snapshot summary fields, and rejects lineage drains that do not replay
  `table.commits-listed` receipt hashes plus compact commit count, sequence, and
  commit-hash summary fields; saved replay verification now prints that compact
  commit-history summary for QueryGraph/operator handoff;
  reused keys with different request hashes now return conflict; failed pointer
  commits now clean up newly written local metadata objects when they do not
  become the table's metadata pointer, and cleanup failures preserve the
  original store/CAS error class with appended cleanup context instead of
  masking the commit failure; REST commits now reject metadata-object writes
  whose target equals the table's current metadata pointer before touching
  object storage, preventing current metadata files from being overwritten
  before CAS/store validation; metadata-write plans now also fail closed if
  they require a metadata object write but do not carry a concrete new metadata
  location; metadata-object commit locations are now bound to the table's
  matched storage profile prefix before object storage is touched; metadata
  object writes and cleanup now route through `object_store::parse_url_opts`,
  preserving local `file://` behavior while moving the writer toward configured
  object-store backends.*
- **P4 — Semantic catalog graph (F6).** Emit the bounded typed taxonomy
  (Namespace/Table/Column/Snapshot/Policy/Principal/ScanPlan/Commit) through the
  outbox into Grust; keep file-granularity as metadata-as-data. Then OpenLineage
  transport + TypeDID attestation on the same events. *Started:
  `catalog.config-read` replay now emits a warehouse-scoped graph event and
  LakeCat OpenLineage receipt for the standard Iceberg REST config entrypoint;
  `namespace.created` outbox replay now emits a catalog-facing `Namespace` graph
  event with a stable namespace subject and the same authorization payload used
  for lineage; `namespace.listed` and `namespace.loaded` replay now carry
  standard namespace reads into warehouse/namespace-scoped graph events plus
  LakeCat OpenLineage receipts; `policy-binding.upserted` replay now emits a
  catalog-facing `Policy` graph event with stable policy subject, ODRL material,
  and authorization payload; `table.scan-planned` and `table.scan-tasks-fetched`
  replay now emit stable catalog-facing `ScanPlan` graph events with governed
  read restrictions; `table.commit` replay now emits stable catalog-facing
  `Commit` graph events keyed by table and committed sequence; `table.restored`
  replay now emits a catalog-facing Table graph event plus the existing
  OpenLineage restore receipt while leaving restore-specific graph taxonomy to
  Grust; non-anonymous resolved principals now replay as stable catalog-facing
  `Principal` graph events; table metadata graph summaries now replay as stable
  catalog-facing `Column` and `Snapshot` graph events; policy-binding,
  project, and warehouse upserts now also emit LakeCat lineage/OpenLineage
  receipts from the same durable outbox replay; management list reads for
  policy bindings, projects, servers, storage profiles, and warehouses replay
  into LakeCat OpenLineage receipts without adding list-specific graph nodes in
  LakeCat, and lineage-drain summaries expose compact list counts/scope for
  QueryGraph verification; lineage-drain summaries also expose compact
  scan/fetch/delete task counts so QueryGraph can verify the governed
  Sail-planned read path without parsing raw lineage payloads; QGLake
  acceptance now establishes a durable
  server/project/warehouse tenant spine, performs governed server, project,
  warehouse, policy-list, and storage-profile-list reads, and requires matching
  `server.listed`, `project.listed`, `warehouse.listed`,
  `policy-binding.listed`, and `storage-profile.listed` replay evidence; table
  events continue through the Grust-owned event graph adapter.*
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
  events and `server.upserted` now emits LakeCat lineage/OpenLineage receipts
  while leaving reusable graph hierarchy work to Grust; storage-profile writes
  now emit lineage/OpenLineage receipts for credential-root changes from durable
  outbox replay; management routes now use the requested warehouse instead of
  the configured default, Iceberg REST routes now accept a warehouse prefix only
  after resolving a durable `WarehouseRecord`, and project-scoped management
  routes can list/upsert warehouses under their durable project while keeping
  the unprefixed default-warehouse compatibility path; credential-vend attempts
  with fine-grained restrictions now fail closed into governed Sail-planned
  reads before any secret resolver is called; all accepted production secret-ref
  schemes are now exercised through TypeSec authorization before failing closed
  when no resolver backend is configured; governed view list/upsert/drop
  management endpoints now persist and delete durable `ViewRecord` values in
  memory and Turso and emit audited `view.*` events; `ViewRecord` now carries
  durable typed columns and store-assigned `view-version` counters,
  warehouse-prefixed catalog REST aliases now list, load, upsert, and drop those
  durable views, and the in-process Sail provider can create/load/list/drop them
  as `TableKind::View` statuses; governed namespace
  list/load/drop is now wired on unprefixed and warehouse-prefixed Iceberg REST
  paths and through the in-process Sail `CatalogProvider` `drop_database` path,
  with non-empty guards across tables, views, and policy bindings plus audited
  `namespace.listed`, `namespace.loaded`, and `namespace.dropped` graph/lineage
  projection on the REST path; `view.listed` replay emits namespace-scoped graph
  and OpenLineage evidence, while
  `view.upserted`, `view.loaded`, and `view.dropped` outbox replay emits
  catalog-facing View graph events and LakeCat OpenLineage receipts; lineage
  drain summaries expose compact view warehouse/namespace/name/stable-id/version
  evidence and QueryGraph bootstrap replay exposes matching view-version receipt
  hashes for QGLake acceptance, and a governed management read endpoint exposes
  compact view-version receipt chains for QueryGraph/operators; view drops now
  append a compact tombstone receipt that preserves the last durable view
  version and content hash after the current view row is removed, while leaving
  reusable view graph topology to Grust; governed view receipt-chain reads now
  replay into lineage, and a namespace-level `view-version-receipt-chains`
  read groups active and tombstoned chains for QueryGraph/operators so QGLake
  can create, bootstrap, drop, receipt-check by view and namespace, and drain a
  transient view while rejecting dropped-view replay that lacks either per-view
  tombstone receipt evidence or namespace-level receipt-chain evidence;
  view-version receipts now carry `previous-receipt-hash` links so upsert and
  drop receipts form a compact hash chain over the catalog-facing version
  history, and namespace receipt-chain reads expose deterministic `chain-hash`
  proofs plus `chain-verified` link validation that lineage-drain summaries
  replay for QGLake/QueryGraph acceptance;
  QueryGraph bootstrap now exports those stored views with manifest-covered OSI
  hashes, typed view columns, durable view versions, compact view receipt
  evidence in the import compatibility contract, view-aware graph edges, and
  OpenLineage view counts, and QGLake lineage-drain acceptance rejects replay
  that does not preserve the accepted view version or its compact durable
  receipt hash; `lakecat-cli qglake-verify-replay` now runs the same handoff
  proof against saved bootstrap and lineage-drain artifacts; the live
  Sail-backed fixture now emits canonical Iceberg REST `not-eq` predicates,
  preserves TypeSec request-identity evidence from the current
  `authorization-receipt.context` shape, deduplicates shared namespace nodes in
  the QueryGraph graph envelope, and has been verified through QueryGraph's
  Rust `lakecat-verify` and `lakecat-import` commands over the same saved
  bundle. Full Iceberg view history and commit semantics remain pending.*
- **P6 — Reproducibility (F10) + typed v4 (F9).** Land the Sail helper commits
  upstream (or pin a published Sail) and re-enable automatic CI; converge on
  typed v4 metadata once `sail-iceberg` provides it. The current LakeCat bridge
  now has focused format-version 4 fixtures for JSON summary extraction,
  manifest-list scan planning, and stable commit-requirement validation, but
  pruning and typed metadata-tree semantics remain Sail-owned follow-up work.
  A local dependency-contract audit now checks the versioned Grust/TypeSec path
  pins, the Sail path/patch bridge, and the manual-only CI trigger so F10 drift
  is executable even before automatic CI is re-enabled.

---

## Verdict

LakeCat crossed the line OPUS1 drew: it is a real, governance-gated,
durably-committing Iceberg catalog with the seams pointed the right way and the
boundaries intact. The design bet is being vindicated by the code. The
restriction is now parsed from ODRL, carried into governed scan/credential
receipts, re-applied through plan-task fetch, and used to steer agents onto the
governed Sail-planned path while trusted humans keep audited standard credential
vending with OpenLineage replay evidence. The remaining distance to the GOAL is
short and specific — keep the local QGLake handoff harness in the regular
verification loop when handoff behavior changes, push more read execution
upstream into Sail, keep QGLake's acceptance fixture on real manifest-backed
metadata, and continue tightening the byte-compatible proof for standard
Iceberg clients.
