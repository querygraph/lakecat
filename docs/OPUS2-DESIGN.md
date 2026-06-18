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
│  │     ⛔ missing: ODRL→restriction · masked plan                  │   │
│  │  ┌─────────────────────────────────────────────────────────┐ │   │
│  │  │  ENGINE (Sail)  ✅ Tier-1 provider for commits           │ │   │
│  │  │     ⛔ reads still walk manifests in-process             │ │   │
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
| Identity, capability gate, pointer CAS, audit, outbox, **the restriction** | **LakeCat** | ✅ gate; ⛔ restriction |
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
| F1 | Governed read gates but does not mask | HIGH | OPEN — critical path |
| F2 | ODRL transported, not interpreted; `Delegate` → deny | HIGH | STARTED — shared restriction parser plus TypeSec RBAC policy loading |
| F3 | Commit idempotency unreachable from REST | MEDIUM | STARTED — REST header replay + mismatch guard wired |
| F4 | Metadata written before CAS; no orphan handling/retry | MEDIUM | STARTED — local orphan cleanup |
| F5 | Scans bypass the in-process provider | MEDIUM | STARTED — REST `sail-local` plan and fetch routes now use provider seams |
| F6 | Catalog graph is event breadcrumbs | MEDIUM | OPEN — Grust seam ready |
| F7 | Single warehouse; no Project/Server/View entities | LOW | OPEN |
| F8 | Production secret backends unexercised | LOW | OPEN (fails closed) |
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
     *Started: scan receipts now carry `read-restriction` with allowed columns
     and policy hashes.*
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
  the GOAL's acceptance target; P1 is the only thing it's missing.
- **P3 — Commit hardening (F3, F4).** Wire REST idempotency keys into the
  existing store replay; make metadata writes survive CAS conflict (finalize
  after win, or bounded re-plan + orphan cleanup); generalize the writer beyond
  `file://` to the declared `object_store` backends. *Started: REST commits now
  accept validated `x-lakecat-idempotency-key` values and replay through the
  store idempotency record instead of creating a second pointer-log row; reused
  keys with different request hashes now return conflict; failed pointer commits
  now clean up newly written local metadata objects when they do not become the
  table's metadata pointer.*
- **P4 — Semantic catalog graph (F6).** Emit the bounded typed taxonomy
  (Namespace/Table/Column/Snapshot/Policy/Principal/ScanPlan/Commit) through the
  outbox into Grust; keep file-granularity as metadata-as-data. Then OpenLineage
  transport + TypeDID attestation on the same events.
- **P5 — Tenancy (F7) + production credentials (F8).** Project/Warehouse as
  stored entities with management endpoints; real Vault/AWS/GCP/Azure resolvers
  behind the TypeSec gate. Needed for multi-tenant deployment, not for the demo.
- **P6 — Reproducibility (F10) + typed v4 (F9).** Land the Sail helper commits
  upstream (or pin a published Sail) and re-enable automatic CI; converge on
  typed v4 metadata once `sail-iceberg` provides it.

---

## Verdict

LakeCat crossed the line OPUS1 drew: it is a real, governance-gated,
durably-committing Iceberg catalog with the seams pointed the right way and the
boundaries intact. The design bet is being vindicated by the code. The remaining
distance to the GOAL is short and specific — build *the restriction* and drive it
through a Sail-planned read, then let the QGLake broker prove that a governed
catalog can give agents strictly less than humans while staying byte-compatible
with every standard Iceberg client. Everything needed for that now exists; what's
left is to connect it.
