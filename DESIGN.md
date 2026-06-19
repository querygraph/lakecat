# LakeCat Design

Updated: 2026-06-19

Status: living design. This document supersedes the OPUS review/design notes
that are now archived under `docs/completed/`.

## Purpose

LakeCat is the Rust-native Iceberg-compatible catalog foundation for
QueryGraph. It should preserve the standard Iceberg table boundary while
bringing planning, governance, graph projection, lineage, and agent-facing
evidence as close to the data as possible.

The catalog boundary stays intentionally thin:

- Iceberg REST compatibility, identity, tenancy, namespace/table/view state,
  metadata-pointer compare-and-swap, idempotency, audit, and outbox events live
  in LakeCat.
- Iceberg format behavior, manifest handling, pruning, metadata-as-data, and
  reusable scan-planning helpers belong in Sail.
- Graph schema, taxonomy, projection mechanics, traversal, graph stores, and
  Cypher behavior belong in Grust.
- Governance semantics, policy composition, TypeDID envelopes, capabilities,
  secure agents, and proofs belong in TypeSec.
- QueryGraph is the end-to-end integration target for Croissant, CDIF, OSI,
  ODRL, OpenLineage, agent workflows, and QGLake acceptance.

## Consolidation Note

The historical OPUS files remain available for audit:

- `docs/completed/OPUS1.md`
- `docs/completed/OPUS1-DESIGN.md`
- `docs/completed/OPUS2.md`
- `docs/completed/OPUS2-DESIGN.md`

They should not be treated as active instructions. Their live decisions,
findings, and priorities are merged here as the permanent design record. Current
work should read this file, `AGENTS.md`, `ARCHITECTURE.md`, `GOAL.md`,
`STATUS.md`, and the live code. If those disagree, prefer the live code and
newest status/design entry, then reconcile the docs in the same logical unit.

### Canonical Document Map

Use this map instead of reopening OPUS files for routine work:

| Need | Canonical home |
| --- | --- |
| LakeCat thesis, compatibility rules, OPUS finding closure, and priority plan | `DESIGN.md` |
| Target architecture and repo placement rules | `ARCHITECTURE.md` |
| Durable operating goal and source-of-truth order | `GOAL.md` |
| Agent/commit/verification discipline | `AGENTS.md` |
| Latest completed slices, local verification, blockers, and next work | `STATUS.md` |
| User-facing change record before each commit | `CHANGELOG.md` |
| Book-quality operator narrative and examples | `docs/book/lakecat.md` |
| Historical review provenance only | `docs/completed/OPUS*.md` |

### Archive Policy

The OPUS files are frozen completed reviews, not living design plans. Do not
append new review logs to them, and do not cite them as current authority in new
work. If an archived OPUS detail becomes relevant again, first merge the durable
part into the canonical document above, then implement from the canonical doc.

The only acceptable changes to `docs/completed/OPUS*.md` are archive-maintenance
edits such as fixing links, adding an archive banner, or correcting broken
provenance. New LakeCat design and implementation guidance belongs here or in
the adjacent canonical docs.

## Thesis

Iceberg constrains one layer: how table state is stored and how standard
engines talk to the catalog. Everything QueryGraph needs beyond that layer,
including graph, lineage, governance, semantic projections, and agent receipts,
belongs in the control plane or derived projections.

Keep Iceberg pristine at the boundary. Innovate around it.

```text
Derived / semantic layer
  Grust catalog graph, Croissant, CDIF, OSI, ODRL, OpenLineage
    ^
    | derived from committed state and durable events
Control plane
  identity, tenancy, Capability<A,R>, restriction, CAS, audit, outbox
    ^
    | standard catalog operations and governed privileged paths
Engine layer
  Sail catalog bridge, scan planning, manifest pruning, metadata-as-data
    ^
    | table metadata, manifests, object storage
Iceberg floor
  pristine metadata files, manifests, REST compatibility
```

## Ownership

| Concern | Owner | LakeCat role |
| --- | --- | --- |
| Iceberg REST compatibility | LakeCat + Sail | Serve standard clients and prefer generated Sail models/helpers. |
| Format semantics, manifests, pruning, deletes, metadata-as-data | Sail | Call or upstream reusable APIs; do not fork them locally. |
| Identity, tenancy, pointer CAS, idempotency, audit, outbox | LakeCat | Own durable catalog state and receipts. |
| Policy, TypeDID, capability semantics, secure agents | TypeSec | Ask for decisions/proofs and persist receipts. |
| Catalog graph taxonomy, projection mechanics, stores, Cypher | Grust | Keep only catalog-facing sink boundaries in LakeCat. |
| Croissant, CDIF, OSI, ODRL, OpenLineage composition | LakeCat + QueryGraph | LakeCat emits verifiable contracts; QueryGraph composes higher-level workflows. |
| End-to-end agentic acceptance | QueryGraph | LakeCat supplies bootstrap bundles, handoff evidence, and replayable receipts. |

Dependency direction should remain:

```text
QueryGraph -> LakeCat -> Sail / Grust / TypeSec
```

LakeCat must not import QueryGraph. QueryGraph may consume LakeCat's standard
REST surface, management/bootstrap exports, outbox replay, and QGLake handoff
evidence.

## Compatibility Rules

- Do not fork Iceberg semantics or make normal table access depend on
  non-standard endpoints.
- Keep Iceberg metadata pristine. Business semantics, policies, graph facts,
  lineage, and agent state are derived control-plane or graph data.
- Prefer typed Sail support for v4 work when available. JSON passthrough is a
  compatibility bridge, not the long-term design.
- Raw credential vending is a deliberate audited exception. Governed
  Sail-planned reads are the default for agents and untrusted principals.
- Client-supplied governance filters are hints at most. The server derives the
  effective restriction.

## Critical Path: The Restriction

The central design object is the restriction:

> The server-derived, principal-specific set of allowed columns, row predicate,
> purpose, and credential TTL that a read is narrowed to.

The restriction is derived from policy, never trusted from client input, carried
by the capability, applied by Sail planning/fetch paths, and recorded in the
receipt. It is the bridge between TypeSec authorization and actual Iceberg
scan behavior.

The governed read path must keep this invariant:

```text
policy + principal + purpose
  -> TypeSec decision/proof
  -> LakeCat effective restriction
  -> Capability<principal, action, resource>
  -> Sail-planned scan / fetch-scan-tasks revalidation
  -> audit receipt and QueryGraph evidence
```

When the restriction is complete end to end, QGLake's
metadata-visible/data-denied broker is just one policy shape: metadata remains
visible, data columns are narrowed to none, and the receipt proves the decision.

## Current State

- The durable local spine uses Turso-backed catalog state behind the
  `CatalogStore` contract.
- The commit path has pointer CAS, idempotency, metadata object writes, audit,
  and outbox events.
- The Sail bridge exists as an in-process `CatalogProvider` path for governed
  catalog access; more read execution and v4 typing should still move upstream
  into Sail.
- TypeSec-backed governance is wired through the service layer, with receipts
  and fail-closed credential behavior. Policy-derived credential TTL caps are
  carried into issuance requests, returned credential config, and QGLake
  credential replay/handoff evidence.
- ODRL restrictions are no longer only transported as opaque context; the
  enforceable subset is moving through restrictions and receipts, and
  constraint-form operators now fail closed when they are missing or do not
  mean "use this value as the allowed/narrowing restriction."
- Graph and lineage side effects are moving through bounded catalog events and
  replayable outbox evidence. Grust owns reusable graph behavior.
- QueryGraph bootstrap and QGLake handoff flows now carry table and view
  evidence, view receipt chains, accepted-view chain hashes, graph/import
  proofs, and local verifier coverage.
- Local dependency-contract checks guard published Grust/TypeSec resolution,
  the remaining Sail local path/patch bridge, concrete Sail helper APIs, the
  manual-only CI trigger, and the sibling QueryGraph Rust importer's
  `receipt-chain-hash` handling.
- Automatic cloud CI remains deliberately disabled/manual until local gates are
  known to pass.

## Finding Status

| Finding | Status | Current meaning |
| --- | --- | --- |
| F1 governed reads gate but must narrow | Started | Restrictions now flow through governed planning/fetch proof, but Sail should own more read execution. |
| F2 ODRL transported but not fully interpreted | Started | Enforceable ODRL subset is becoming restriction input; unsupported constraint operators now fail closed; broader composition stays in TypeSec/QueryGraph. |
| F3 REST commit idempotency | Started | Store support exists; REST-visible behavior needs continued hardening. |
| F4 metadata write before CAS orphan handling | Started | Commit hardening exists; cleanup and proof paths still matter. |
| F5 scans bypass in-process provider | Started | Plan/fetch paths are guarded; reusable Sail planner integration remains the target. |
| F6 graph projection still shallow | Started | Catalog graph events are bounded and expanding; reusable taxonomy/query behavior belongs in Grust. |
| F7 tenancy hierarchy not fully routed | Started | Server/project/warehouse/namespace anchors are projected and used in bootstrap. |
| F8 production secret refs | Started | Explicit provider dispatch seams fail closed and receive policy TTL caps; QGLake credential replay now proves the same TTL cap; SDK-backed resolvers beyond configured backends remain pending. |
| F9 v4 JSON passthrough | Open by design | Keep compatibility bridge until typed Sail v4 support is available. |
| F10 sibling dependency drift | Open but guarded | Local dependency-contract audits check Grust/TypeSec, Sail, QueryGraph, and manual CI state. |

## OPUS Closure Map

The OPUS review files are archived, but their findings remain represented by
the live status above and by the priorities below. Use this map when checking
whether an old OPUS item still needs work.

| OPUS item | Current design home | State |
| --- | --- | --- |
| OPUS1 F1 default-feature tests red | Review Gate and local verification | Closed locally; keep default-feature tests in the verification matrix. |
| OPUS1 F2 no authentication / anonymous principal | Critical Path, P1, P5 | Started; typed principals, TypeSec decisions, capability receipts, and credential gates exist. Production TypeDID/key resolution remains a TypeSec/QueryGraph-facing hardening item. |
| OPUS1 F3 no real commit / no metadata pointer CAS | P3 Commit Hardening | Started; Turso and memory stores have pointer CAS, idempotency, audit, pointer logs, outbox evidence, and cleanup hardening. Continue object-store/generalized retry work. |
| OPUS1 F4 no durable local store | Current State, P3, P5 | Closed for the local spine through the Rust `turso` crate behind `CatalogStore`; keep the store contract portable. |
| OPUS1 F5 real engines not activatable from binary | Ownership, P6 | Started; local feature gates wire TypeSec, Grust, Sail, and dependency-contract audits. Keep publish/path drift visible. |
| OPUS1 F6 Sail used as a struct library | P1, P6 | Started; provider seams and Sail-owned helper APIs exist. More read execution, manifest work, pruning, and typed v4 support should continue moving upstream to Sail. |
| OPUS1 F7 plan and implementation drift | Review Gate, STATUS.md, CHANGELOG.md | Guarded by this consolidated design, status updates, and local dependency-contract checks. |
| OPUS1 F8 placeholder graph emission | P4 Semantic Catalog Graph | Started; LakeCat emits bounded catalog-domain events and Grust owns graph mechanics, taxonomy, stores, traversal, and Cypher. |
| OPUS1 F9 fabricated default namespace | P5 Tenancy And Credentials | Started; durable namespace and warehouse-prefixed routing exist. Continue tightening view/history and full tenancy semantics. |
| OPUS1 F10 side effects coupled to request path | P3, P4 | Started; audit/outbox and replayable lineage/graph evidence are core catalog state-change companions. Move remaining side effects toward transactional outbox paths. |
| OPUS1 F11 unauthenticated plan-task tokens / path exposure | P1 QGLake Acceptance | Started; plan/fetch tokens are table-bound and revalidated with server-derived restrictions. Keep path and token evidence audit-safe. |
| OPUS1 F12 v4 JSON passthrough | P6 Reproducibility And V4 | Open by design; JSON passthrough is a bridge until typed Sail v4 support lands. |
| OPUS2 F1 governed read gates but does not narrow | P1 Restriction End To End | Started; restrictions now narrow plan/fetch evidence. Continue pushing reusable read execution into Sail. |
| OPUS2 F2 ODRL transported but not interpreted | P1 Restriction End To End | Started; enforceable ODRL subsets feed restrictions and unsupported operators fail closed. Broader composition belongs in TypeSec and QueryGraph. |
| OPUS2 F3 REST commit idempotency unreachable | P3 Commit Hardening | Started; REST idempotency keys replay through store records with mismatch guards. |
| OPUS2 F4 orphan metadata after CAS failure | P3 Commit Hardening | Started; local cleanup and redacted cleanup evidence exist. Continue generalized object-store cleanup/retry work. |
| OPUS2 F5 scans bypass in-process provider | P1, P6 | Started; REST `sail-local` plan/fetch routes use provider seams, but Sail should own more planner execution. |
| OPUS2 F6 catalog graph is breadcrumbs | P4 Semantic Catalog Graph | Started; keep file-granularity out of graph and use Sail metadata-as-data for file enumeration. |
| OPUS2 F7 tenancy hierarchy durable but not fully routed | P5 Tenancy And Credentials | Started; server/project/warehouse/view anchors and routing are active. Full Iceberg view semantics remain pending. |
| OPUS2 F8 production secret stores unexercised | P5 Tenancy And Credentials | Started; configured production providers dispatch only after TypeSec authorization and preserve TTL caps. SDK-backed resolvers remain pending. |
| OPUS2 F9 v4 JSON passthrough | P6 Reproducibility And V4 | Open by design. |
| OPUS2 F10 sibling dependency drift / manual CI | P6 Reproducibility And V4 | Open but guarded; keep local verification and dependency-contract checks ahead of any cloud CI. |

## OPUS Decisions Kept Permanent

- LakeCat is a catalog and control plane, not a new table format. Standard
  clients keep using Iceberg REST and pristine Iceberg metadata.
- The governed path is the default for agents. Raw credential vending is an
  audited, TypeSec-authorized exception for principals allowed to read directly.
- The restriction is server-owned. Client projection and filters can only
  narrow inside policy-derived columns, predicates, purpose, and credential TTL.
- Sail owns reusable Iceberg and planning work: manifest IO, pruning, deletes,
  metadata-as-data, scan planning, v4 typing, and table-maintenance helpers.
- Grust owns reusable graph mechanics: schema/taxonomy, projection primitives,
  graph stores, traversal, Cypher, and backend-specific algorithms.
- TypeSec owns reusable governance semantics: capability composition, TypeDID,
  secure agents, proofs, and authorization semantics.
- QueryGraph is above LakeCat. It consumes LakeCat evidence, bootstrap bundles,
  outbox replay, OpenLineage, Croissant/CDIF/OSI/ODRL projections, and QGLake
  handoff proofs; LakeCat must not import QueryGraph.
- Graph materialization should stay bounded. Stable semantic entities belong in
  catalog graph events; file-scale facts stay in Iceberg/Sail metadata-as-data.

## Priority Plan

### P1 Restriction End To End

Keep tightening the read path so the effective restriction is derived by the
server, carried by capability, applied by Sail plan/fetch paths, and captured in
receipts. Prefer upstream Sail APIs for any reusable planner or manifest work.

### P2 QGLake Acceptance

Keep the live QGLake handoff harness in the verification loop. QueryGraph must
continue importing LakeCat evidence without losing view receipt-chain hashes,
accepted view versions, graph proof, import proof, or OpenLineage replay
anchors. Credential replay must preserve the policy-derived TTL cap in both the
captured LakeCat replay evidence and compact handoff summary.

### P3 Commit Hardening

Continue hardening REST-visible idempotency, metadata object orphan cleanup, CAS
conflict receipts, and recovery behavior. Catalog state changes should not lose
outbox side effects.

### P4 Semantic Catalog Graph

Move graph mechanics to Grust and keep LakeCat's role to typed catalog-domain
events and sinks. Do not add traversal, schema reasoning, or graph query
behavior to LakeCat.

### P5 Tenancy And Credentials

Keep management hierarchy and credential roots durable and replayable. Raw
credential vending remains an audited exception behind TypeSec authorization;
restricted Sail-planned reads are the safer default.

### P6 Reproducibility And V4

Keep local verification ahead of cloud CI. Land reusable Sail helpers upstream
or pin published versions before removing local path/patch bridges. Replace v4
JSON passthrough with typed Sail support when Sail exposes stable APIs.

## Review Gate

Before implementing a slice:

- Read `AGENTS.md`, `STATUS.md`, this `DESIGN.md`, `ARCHITECTURE.md`,
  `GOAL.md`, and the relevant live code.
- Decide whether the work belongs in LakeCat or should be pushed to Sail,
  Grust, TypeSec, or QueryGraph.
- Update `CHANGELOG.md` before committing.
- Update the book when behavior, public workflow, architecture, or acceptance
  evidence changes.
- Run the focused local gates first. Include
  `scripts/check-local-dependency-contract.sh` when dependency boundaries,
  sibling APIs, CI policy, or QueryGraph handoff/import evidence changes.
- Commit each logical unit only after the corresponding docs/status are
  reconciled.
