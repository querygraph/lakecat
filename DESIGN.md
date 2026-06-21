# LakeCat Design

Updated: 2026-06-21

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

## OPUS Consolidation

The OPUS corpus is consolidated into this living design and the adjacent
canonical docs. The archived files under `docs/completed/` preserve review
provenance only:

- `docs/completed/OPUS1.md`
- `docs/completed/OPUS1-DESIGN.md`
- `docs/completed/OPUS2.md`
- `docs/completed/OPUS2-DESIGN.md`

Current work should read this file, `ARCHITECTURE.md`, `GOAL.md`, `AGENTS.md`,
`STATUS.md`, `CHANGELOG.md`, the book, and the live code. If those disagree,
prefer the live code and newest status/design entry, then reconcile the docs in
the same logical unit.

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

### Consolidated Source Ledger

The durable OPUS sections have been absorbed into the active docs as follows.
This ledger is the routing table for future reconciliation, so routine work
should not reopen archived OPUS files just to find the current plan.

| Archived source | Durable material absorbed | Canonical home |
| --- | --- | --- |
| `OPUS1.md` verification notes and findings | The original scaffold risk list: default-feature gates, authentication, durable pointer state, CAS, real store, Sail delegation, graph emission, side effects, plan tokens, and v4 bridge posture. | `Review Log And Working Plan`, `Finding Status`, `OPUS Closure Map`, `Priority Plan`, `Review Gate` |
| `OPUS1.md` proposed architecture and milestones | Thin catalog boundary, Sail-heavy engine plan, TypeSec capability model, Grust semantic index, lineage/attestation, QueryGraph acceptance, and upstream-to-Sail work. | `Purpose`, `Ownership`, `Compatibility Rules`, `OPUS Decisions Kept Permanent`, `Priority Plan`; see also `ARCHITECTURE.md` |
| `OPUS1-DESIGN.md` design assessment | Iceberg compatibility floor, derived semantic/control plane, REST server-side planning, row lineage, metadata-as-data, credential-vending tension, Tier-1 Sail provider sketch, and anti-patterns. | `Thesis`, `Critical Path: The Restriction`, `Compatibility Rules`, `OPUS Decisions Kept Permanent`, `Review Gate` |
| `OPUS1-DESIGN.md` working plan | The early dev-manager review gate, verification discipline, Turso pivot endorsement, QueryGraph OSI cleanup, Grust/TypeSec reconciliation, and priority ordering. | `Review Log And Working Plan`, `Current Dev-Manager View`, `Priority Plan`, `Review Gate`, `STATUS.md` |
| `OPUS2.md` second review | Current catalog-spine assessment after Turso/CAS/auth/outbox/Sail-provider work, OPUS1 closure state, and successor findings F1-F10. | `OPUS2 Review Baseline`, `Current State`, `Finding Status`, `OPUS Closure Map` |
| `OPUS2-DESIGN.md` restriction-first plan | The restriction as the binding governed-read object, ODRL-to-restriction path, Sail-planned enforcement, QueryGraph/QGLake acceptance, repo division of labor, and anti-pattern updates. | `Critical Path: The Restriction`, `Current Dev-Manager View`, `Priority Plan`, `OPUS Decisions Kept Permanent` |

### Adjacent Document Merge Ledger

Some OPUS material intentionally belongs outside this file because it is either
operational, architectural, narrative, or historical. Treat the table below as
the canonical merge record for those pieces.

| OPUS material | Active home | Why it lives there |
| --- | --- | --- |
| Repo division of labor, Sail/Grust/TypeSec ownership, and high-level system shape | `ARCHITECTURE.md` | It is target architecture, not review history. |
| Durable goal, source-of-truth order, local-first/cloud policy, book workflow, and pinned agent guidance | `GOAL.md` | It controls how the ongoing goal resumes after context loss. |
| Agent commit discipline, feature-gate expectations, graph/Sail/TypeSec placement rules, and Turso preference | `AGENTS.md` | It is executable guidance for future agents working in this repo. |
| Latest completed slices, verification evidence, blockers, and next recommended work | `STATUS.md` | It changes after each logical unit and should not be buried in design prose. |
| Public/operator explanation, examples, and QGLake workflow narrative | `docs/book/lakecat.md` | It is the reader-facing book and must evolve with user workflows. |
| Historical verification tables, original line references, and review provenance | `docs/completed/OPUS*.md` | It is audit history; it is not the active backlog. |

When old OPUS wording is useful again, merge the durable rule into the active
home above and leave the archived source frozen. Do not add a second summary doc
beside this ledger.

### Archive Lock

The root tree should contain no active `OPUS*.md` files. Do not create
`OPUS3.md`, revive root-level OPUS notes, or append new working plans to the
archived OPUS files. A future review should update the canonical document that
owns the decision and may add a completed-review artifact under `docs/completed/`
only after the durable guidance has landed in the active docs.

### Consolidation Audit

The OPUS archive was rechecked on 2026-06-20. The active tree has no
root-level `OPUS*.md` files, and the completed-review archive contains exactly
the four historical OPUS files listed above. Each archived file has an archive
banner pointing back to this design document.

Treat that shape as the invariant:

- `git ls-files 'OPUS*.md'` should return no files.
- `rg --files -g 'OPUS*.md' -g '!docs/completed/**'` should return no files.
- `git ls-files 'docs/completed/OPUS*.md'` should return only
  `OPUS1.md`, `OPUS1-DESIGN.md`, `OPUS2.md`, and `OPUS2-DESIGN.md`.

### Consolidated OPUS Digest

The durable OPUS guidance now collapses to these operating rules:

- Keep LakeCat standard at the Iceberg boundary and innovative in the control
  plane. Standard clients should succeed through normal Iceberg REST behavior;
  QueryGraph-specific semantics ride derived evidence, graph projections,
  lineage, and governed workflows.
- Treat the restriction as the binding governed-read object. It is derived by
  the server from TypeSec/ODRL policy, carried by capabilities and receipts,
  applied by Sail plan/fetch paths, and replayed into QGLake evidence.
- Keep the catalog spine durable and auditable: pointer CAS, idempotency,
  metadata-object handling, audit, outbox, redaction, and replay proof are
  LakeCat-owned responsibilities.
- Keep the repo boundaries active. Reusable Iceberg/planning work goes to Sail;
  reusable graph taxonomy/query/storage work goes to Grust; reusable governance,
  TypeDID, capability, and secure-agent semantics go to TypeSec.
- Use QGLake as the acceptance loop. Bootstrap, scan/fetch, credentials, views,
  graph/import, OpenLineage, and captured replay proofs should reject drift
  before a slice is considered done.
- Keep cloud automation manual until local verification is green. The OPUS
  process finding is not "add more CI"; it is "prove locally first, then turn on
  automation only when it is trustworthy."

Archive health can be checked with:

```text
git ls-files 'OPUS*.md'
git ls-files 'docs/completed/OPUS*.md'
rg --files -g 'OPUS*.md' -g '!docs/completed/**'
rg --files docs/completed -g 'OPUS*.md'
```

## Review Log And Working Plan

This section consolidates the OPUS review history into the active
dev-manager view. It is the place to look for the durable conclusion of the
reviews; `docs/completed/OPUS*.md` is provenance only.

### OPUS1 Review Baseline

OPUS1 reviewed the first LakeCat scaffold. The important verdict was that the
seams were right but the catalog was not yet load-bearing. The review found
missing authentication, missing durable pointer state, missing metadata-pointer
CAS, inactive real integration engines, shallow graph emission, and plan/code
drift. The durable lesson from OPUS1 is still active: keep the trait seams,
but never mistake a seam for the behavior it promises.

The OPUS1 design companion made the core architecture durable:

- Iceberg remains the compatibility floor.
- QueryGraph innovation lives in the control plane, derived graph/lineage
  projections, and governed Sail-planned reads.
- The governed path is the default for agents; raw credential vending is a
  deliberate audited exception.
- Sail is the natural home for reusable scan planning, manifest work, pruning,
  and typed v4 support.
- Grust owns reusable graph mechanics; LakeCat emits only catalog-facing graph
  facts and sinks.
- TypeSec owns policy, capabilities, TypeDID, secure-agent, and authorization
  semantics.

### OPUS2 Review Baseline

OPUS2 reviewed the repo after the catalog spine had landed. The headline moved:
LakeCat had become a real authenticated, durably committing, CAS-correct,
governance-gated Iceberg REST catalog with a CLI, Turso-backed state,
idempotency, audit, outbox, an in-process Sail provider, and feature-gated
TypeSec/Grust/Sail integrations. The frontier moved from "is it a catalog?" to
"is the governed path narrow, replayable, and accepted by QueryGraph?"

The OPUS2 design companion named the binding object: the restriction. The
restriction is now the permanent design object for governed reads and credential
decisions. It must be parsed from policy, carried in the capability and receipt,
applied by Sail planning/fetch paths, and replayed into QueryGraph evidence.

### Current Dev-Manager View

The current working plan is:

1. Keep closing the restriction loop. The route, provider, credential, and
   handoff paths already carry ODRL-derived allowed columns, row predicates,
   purpose, credential TTL, policy hashes, and re-applied fetch evidence. The
   default plan/fetch/credential routes fail closed on malformed active ODRL,
   including malformed JSON-LD allowed-column lists, before Sail or issuer side
   effects. The next reusable read-execution work should move upstream to Sail
   instead of growing LakeCat-local planning code.
2. Keep QGLake handoff as the acceptance gate. A change that affects bootstrap,
   scan/fetch proof, credential proof, view receipt chains, graph/import hashes,
   OpenLineage replay, captured output hashes, or QueryGraph import compatibility
   must update the local handoff verifier and the book in the same unit. View
   receipt-chain checks must reject invalid chain heads, forged
   previous-receipt links, unsupported operations, skipped versions, and
   tombstones that advance the durable view version.
3. Keep commit hardening focused on REST-visible correctness: idempotency
   replay, metadata object create-only writes, CAS conflict evidence, orphan
   cleanup, object-store portability, and redacted operator-facing errors.
   Backend object-store error details should be represented by hash evidence,
   not raw paths, bucket/object names, or configuration text. Metadata object
   writes must target child objects under the selected storage profile root, not
   the root itself. Store setup failures, including invalid metadata URI parsing
   and unsupported backend configuration, must follow the same hash-only error
   evidence rule.
4. Keep the graph bounded. LakeCat should emit stable catalog-domain facts for
   Server, Project, Warehouse, Namespace, Table, View, Column, Snapshot, Policy,
   StorageProfile, Principal, ScanPlan, Commit, and lineage runs. Traversal,
   graph query, taxonomy evolution, backend storage, Cypher, and algorithms go
   to Grust.
5. Keep tenancy and credentials replayable. Durable server/project/warehouse,
   namespace, view, policy, storage-profile, and credential-root changes should
   create audit/outbox evidence. Secret references and storage roots must stay
   redacted in replay, represented by provider labels, presence flags, and
   content hashes such as `location-prefix-hash`; validation failures should
   follow the same hash-only rule for storage roots, secret references,
   public-config keys, and production resolver parse failures. Guarded view
   mutations must reject invalid expected-version values before changing active
   view state or appending view-version receipts.
6. Keep reproducibility ahead of integration claims. Run local gates before
   commit, keep cloud CI manual/disabled until it is known green, use published
   Grust/TypeSec crates when available, and keep any Sail path/patch bridge
   explicit until reusable Sail helpers are landed or pinned.

### Done-State Expectations

A LakeCat slice is complete only when the code, canonical docs, status,
changelog, and relevant book/operator text agree. If a slice touches QueryGraph
handoff semantics, the compact handoff verifier should reject missing or drifted
evidence before the slice is considered done. If a slice touches Sail, Grust,
TypeSec, or QueryGraph boundaries, either push the reusable work to that repo or
record the local/published dependency state explicitly.

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
  composed to the tightest value, carried into issuance requests, returned
  credential config, and QGLake credential replay/handoff evidence.
- ODRL restrictions are no longer only transported as opaque context; the
  enforceable subset is moving through restrictions and receipts, and
  constraint-form operators and right operands now fail closed when they are
  missing or do not mean "use this value as the allowed/narrowing restriction."
  The parser accepts camel, kebab, and prefixed JSON-LD operand keys for this
  bounded subset, plus compact JSON-LD `@id` term objects for constraint
  operands/operators and `@value`/`@list` right operands for bounded
  allowed-column, purpose, and credential-TTL values, without growing LakeCat
  into a full ODRL reasoner.
  Purpose composition also fails closed unless all enforced policy material
  agrees on the same purpose.
- Graph and lineage side effects are moving through bounded catalog events and
  replayable outbox evidence. Drains fail rather than reporting partial
  acknowledgement success when the store does not mark the whole projected
  batch delivered. Drains also reject duplicate pending event IDs before
  projection, with hash-only duplicate evidence, so corrupted pending batches
  cannot duplicate downstream side effects. Grust owns reusable graph behavior.
- QueryGraph bootstrap and QGLake handoff flows now carry table and view
  evidence, view receipt chains, accepted-view chain hashes, graph/import
  proofs, credential storage-scope hashes, and local verifier coverage.
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
| F2 ODRL transported but not fully interpreted | Started | Enforceable ODRL subset is becoming restriction input; unsupported constraint operators and missing right operands now fail closed across camel, kebab, prefixed JSON-LD operand keys, and compact `@id` term objects; broader composition stays in TypeSec/QueryGraph. |
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
receipts. Scan evidence should preserve both requested and effective
projection/stat metadata so replay can prove what policy narrowed. Outbox
admission now requires top-level scan read restrictions and authorization
receipt read-restriction contexts to match, with explicit planned and fetched
scan drift coverage at the same policy-hash evidence strength. Governed scan
replay should continue failing when effective projection/stat evidence drifts
outside allowed columns. Empty governed `allowed-columns` evidence should fail
closed for planned and fetched replay like live scan planning instead of
becoming replay-time unrestricted access.
Prefer upstream Sail APIs for any reusable planner or manifest work.

### P2 QGLake Acceptance

Keep the live QGLake handoff harness in the verification loop. QueryGraph must
continue importing LakeCat evidence without losing view receipt-chain hashes,
accepted view versions, graph proof, import proof, or OpenLineage replay
anchors. QueryGraph bootstrap replay must bind table artifacts to
`verified-tables`, and view artifacts plus view-version receipts to
`verified-views`, so saved bootstrap evidence cannot splice semantic artifacts
or receipt chains across manifests. Those verified table/view manifests must
also be duplicate-free at outbox admission and compact handoff verification, so
QueryGraph bootstrap replay and archived handoffs cannot inflate counts by
repeating accepted stable IDs. Scan replay must preserve the
server-derived purpose and
policy-derived TTL cap in both captured LakeCat replay text and compact handoff
proof. Credential replay must preserve the policy-derived TTL cap and redacted
storage-scope hash in both the captured LakeCat replay evidence and compact
handoff summary. Credential-vend replay should fail when top-level
read-restriction evidence or `lakecat:raw-credential-exception` evidence drifts
from the authorization receipt context, or when returned credential evidence
or nested storage-profile warehouse / top-level storage-profile id /
secret-reference evidence drifts from the catalog-derived storage profile,
principal, governed-read, and TTL fields. Returned credential replay evidence
must also surface redacted `credentialPrefixHashes` and keep them
count-aligned, full SHA-256-shaped, and duplicate-free by `prefix-hash` in both
raw lineage drains and compact handoff summaries, so credential counts cannot
be inflated with repeated redacted credential entries. Captured LakeCat replay
JSON must then match the compact credential proof for those prefix hashes, so
an archived handoff cannot pair a valid summary with drifted credential replay
output.

View receipt-chain proof must remain a structural proof, not just a bag of
hashes: the first receipt must be a version-1 upsert without previous-link
fields, and each later link must point at the previous receipt hash, use a
supported operation, and preserve the expected version transition. Service
replay must reject duplicate receipt, drop-receipt, and chain hash arrays
before projection so later compact QGLake proof cannot inherit inflated
view-history evidence.
Lineage-drain replay summaries must also stay bound to the drain-level event
type manifest: a compact QGLake handoff cannot include a replay summary for an
event type that the drain did not declare as delivered, and repeated event
types must match replay summary multiplicity rather than only set membership.
Accepted lineage-drain artifacts must also reconcile their top-level
`delivered`, `eventTypes`, `graphEvents`, and `lineageEvents` totals with the
actual replay summary array before the handoff can be treated as verified.
Saved `lakecatHandoffVerifyOutput` artifacts must stay bound to the archived
lineage-drain artifact as well, including delivered count, event type manifest,
graph event count, and lineage event count. Compact table commit-history proof
must preserve the same duplicate-free commit-hash invariant as service replay,
so archived QueryGraph handoff summaries cannot inflate pointer-log evidence by
repeating a valid commit hash.
Raw lineage-drain replay summaries and compact handoff summaries must both keep
replay, OpenLineage, view receipt, and view receipt-chain hash arrays
duplicate-free as well as SHA-256-shaped, so source replay and archived proof
cannot inflate bootstrap, scan, management, commit-history, view,
storage-profile, or credential evidence by repeating a valid digest.
Compact management proof must preserve the same duplicate-free ID invariant as
service replay, so saved QGLake summaries and lineage-drain artifacts cannot
inflate server, project, warehouse, policy, or storage-profile reads by
repeating valid control-plane identities.

### P3 Commit Hardening

Continue hardening REST-visible idempotency, metadata object orphan cleanup, CAS
conflict receipts, and recovery behavior. Metadata-object writes must be
create-only child objects under the selected storage profile, never overwrites
of the current pointer, existing objects, or the storage root itself; rejection
evidence stays hash-only for both the submitted metadata location and storage
profile root, without echoing the storage-profile id. Metadata object-store
setup failures should likewise expose only metadata-location and backend-error
hashes, not raw URI parse text, schemes, or backend diagnostics. Catalog state
changes should not lose outbox side effects. Table commit-history replay must
carry positive, strictly increasing sequence numbers and duplicate-free commit
hashes before projection, so pointer-log evidence cannot be duplicated or
reordered between the catalog and QGLake proof.
Pending outbox replay should stay deterministic across embedded and Turso stores, ordered by
`created_at,event_id`, with batch limits applied after that order and with
duplicate-safe delivery accounting. Draining should acknowledge delivery only
after every projection in the batch succeeds, leaving events pending for retry
when graph or lineage projection fails.

### P4 Semantic Catalog Graph

Move graph mechanics to Grust and keep LakeCat's role to typed catalog-domain
events and sinks. Do not add traversal, schema reasoning, or graph query
behavior to LakeCat.

### P5 Tenancy And Credentials

Keep management hierarchy and credential roots durable and replayable. Raw
credential vending remains an audited exception behind TypeSec authorization;
restricted Sail-planned reads are the safer default. Any configured
secret-manager or credential issuer backend must return credentials scoped no
broader than the LakeCat storage profile that selected it; scope-rejection
evidence should stay hash-only and must not emit credential replay records.
Credential responses should carry catalog-derived secret-ref provider evidence
when a storage profile uses an external secret reference, and backend-supplied
provider evidence must be replaced rather than trusted. Credential-vend replay
must also reject response evidence whose secret-ref provider drifts from the
selected storage profile before any graph or OpenLineage sink observes it.
Management-list replay should carry count-aligned, syntactically valid,
duplicate-free ID arrays before projection, so compact QueryGraph proof cannot
inflate server, project, warehouse, policy, or storage-profile reads with
repeated identities.

View mutation guards must remain positive, store-assigned version checks so
QueryGraph receipt chains cannot be extended by invalid guarded requests.

### P6 Reproducibility And V4

Keep local verification ahead of cloud CI. Land reusable Sail helpers upstream
or pin published versions before removing local path/patch bridges. Replace v4
JSON passthrough with typed Sail support when Sail exposes stable APIs.
Catalog config should advertise the current bridge honestly:
`lakecat.format.v4=extension-ready`,
`lakecat.format.v4.bridge=json-passthrough`, and
`lakecat.format.v4.typed-sail=unavailable` until typed Sail v4 support exists.
The bridge should still preserve Iceberg REST compatibility for the Sail
metadata it can already decode: manifest expansion must encode null partition
slots and nested Sail literals as JSON instead of treating those partition
tuples as unsupported.
Replay evidence for those defaults must stay unambiguous: defaults are
structured string key/value entries, duplicate keys are rejected, and stale or
contradictory v4 bridge claims fail before graph or OpenLineage projection.
The local dependency contract is the guardrail while cloud CI is manual-only:
it must reject automatic triggers across every GitHub workflow file, not just
the primary CI workflow, including compact, block-list, inline-map, and quoted
YAML trigger forms, before any pushed slice depends on cloud feedback. The
guard should inspect actual `on:` trigger declarations while allowing harmless
workflow keys such as job ids that happen to share event names.

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
