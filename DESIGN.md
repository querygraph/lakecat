# LakeCat Design

Updated: 2026-06-22

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
   Turso table rows and idempotency responses must bind decoded JSON back to
   the row/query table identity before returning standard table records or
   committing over the row.
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
   public-config keys, and production resolver parse failures. Turso
   server/project/warehouse reads must bind decoded JSON back to the selecting
   row identity before returning tenant-root inventory for QueryGraph
   bootstrap or management proof. Turso namespace reads must bind decoded JSON
   back to the selected warehouse row and namespace path before returning or
   dropping standard namespace state. Turso
   policy-binding reads must bind decoded JSON back to the row/query warehouse
   and policy id before matching policies for tables. Turso storage-profile
   reads must likewise bind decoded JSON back to the row/query warehouse and
   profile id before credential-root matching. Guarded view mutations
   must reject invalid expected-version values before changing active view state
   or appending view-version receipts, and memory/Turso mutation paths must
   validate the existing receipt chain before appending a new view receipt so
   corrupted durable history cannot be extended. Memory/Turso keyed active-view
   reads and Turso view-list reads must also bind decoded view JSON back to the
   selected warehouse, namespace, and view identity before returning, updating,
   or dropping active view state. Turso receipt reads and mutation-chain lookups
   must also verify decoded receipt JSON against the row/query warehouse,
   namespace, and view scope before returning or extending durable view-history
   evidence.
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

### First-Release Ledger

The first release should be cut around behavior that is already locally
verifiable, not around every long-term architecture ambition. Treat this ledger
as the release-scope map until it is replaced by an explicit release checklist.

Release-blocking scope:

- Standard Iceberg REST compatibility for catalog config, namespaces, tables,
  metadata-pointer commits, table loads, and warehouse-prefixed routing must
  remain green through the local release gate.
- The Rust service spine, `CatalogStore` seam, Turso-backed local store, memory
  store, pointer CAS, idempotency records, pointer logs, audit rows, and outbox
  rows must remain covered by default, Turso, and all-features tests.
- Replay admission must fail before graph/OpenLineage projection for malformed
  table, namespace, view, management, credential, scan, commit, config, and
  QueryGraph bootstrap evidence that current producers can emit.
- QGLake handoff must keep proving bootstrap, governed scan/fetch, credential,
  management, view receipt-chain, table commit-history, OpenLineage, and
  QueryGraph import evidence with saved artifact hashes.
- The book, README, `STATUS.md`, `CHANGELOG.md`, and this design must describe
  the same standard-vs-extension posture and the same local verification path.

Release-deferred scope:

- Typed Iceberg v4 semantics remain Sail-owned future work. LakeCat may
  advertise `extension-ready` JSON passthrough with
  `typed-sail=unavailable`, but it must not claim settled typed v4 support.
- Cloud SDK-backed secret managers beyond the current Vault and file-backed
  provider roots remain future work. Existing secret-ref proof must stay
  redacted and TypeSec-gated.
- Reusable graph taxonomy, traversal, Cypher, graph stores, and algorithms
  remain Grust-owned. LakeCat keeps only catalog-facing projection/sink
  contracts.
- Full QueryGraph product semantics, Croissant/CDIF/OSI/ODRL composition, and
  agentic reasoning remain QueryGraph/TypeSec responsibilities above the
  catalog substrate.
- Automatic cloud CI stays manual-only until the full local release gate is
  boringly green and any sibling dependency bridge is explicit or published.

Authoritative first-release evidence:

- `scripts/check-release-readiness.sh` is the broad local proof.
- `scripts/check-release-readiness.sh --quick` is the narrow-slice smoke proof.
- `scripts/qglake-handoff-local.sh` is the end-to-end LakeCat to QueryGraph
  handoff proof.
- `docs/book/build.sh` proves the reader-facing book artifacts match the
  current source.
- `scripts/check-local-dependency-contract.sh` proves the Grust/TypeSec/Sail,
  QueryGraph, and CI-trigger assumptions still match the current repo.

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
  into a full ODRL reasoner. Allowed-column lists must be non-empty and
  nonblank, and purposes must be nonblank before the derived `ReadRestriction`
  can reach credential issuance or governed Sail planning/fetch paths.
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
| F2 ODRL transported but not fully interpreted | Started | Enforceable ODRL subset is becoming restriction input; unsupported constraint operators, missing right operands, blank purposes, and empty/blank allowed-column lists now fail closed across camel, kebab, prefixed JSON-LD operand keys, and compact JSON-LD terms; broader composition stays in TypeSec/QueryGraph. |
| F3 REST commit idempotency | Started | Store support exists; REST-visible behavior needs continued hardening. |
| F4 metadata write before CAS orphan handling | Started | Commit hardening exists; cleanup and proof paths still matter. |
| F5 scans bypass in-process provider | Started | Plan/fetch paths are guarded; reusable Sail planner integration remains the target. |
| F6 graph projection still shallow | Started | Catalog graph events are bounded and expanding; reusable taxonomy/query behavior belongs in Grust. |
| F7 tenancy hierarchy not fully routed | Started | Server/project/warehouse/namespace anchors are projected and used in bootstrap. |
| F8 production secret refs | Started | Explicit provider dispatch seams fail closed and receive policy TTL caps; QGLake credential replay now proves the same TTL cap; built-in Vault and file-backed AWS/GCP/Azure-style providers exist, while cloud SDK-backed resolvers remain pending. |
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
| OPUS2 F8 production secret stores unexercised | P5 Tenancy And Credentials | Started; configured production providers dispatch only after TypeSec authorization and preserve TTL caps. Vault and file-backed provider roots are built in; cloud SDK-backed resolvers remain pending. |
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
Governed `row-predicate` evidence must also remain structurally meaningful:
compact and raw replay should reject empty predicate objects, blank predicate
types, and term-based predicates that omit the narrowed term/value evidence.
Requested/effective projection and stats-field evidence should likewise stay
non-empty, non-blank, and duplicate-free before narrowing proof is accepted;
present-but-empty scan proof arrays are malformed, not an implicit unrestricted
projection.
Service outbox admission must enforce the same field-array shape before graph
or OpenLineage projection, not only in later CLI replay verification. The
service boundary now also closes governed top-level and authorization-receipt
read-restriction objects over `allowed-columns`, `row-predicate`, `purpose`,
`policy-hashes`, and `max-credential-ttl-seconds`, and closes nested
row-predicate objects over `type`, `term`, and `value`, so scan replay cannot
carry unverified restriction or predicate claims into graph, OpenLineage, or
later QGLake proof.
Scan-planned and scan-tasks-fetched outbox admission must also reject missing
or malformed `row-predicate` proof whenever governed read-restriction evidence
is present, and fetched `required-filters` proof must exactly preserve that
row predicate before the event is acknowledged.
Planned and fetched scan outbox admission now also rejects governed
read-restriction evidence whose purpose is missing/blank or whose
`max-credential-ttl-seconds` cap is missing or non-positive, so the service
boundary enforces the purpose/TTL proof QGLake later verifies.
Read-restriction `policy-hashes` must remain non-empty, full SHA-256-shaped,
and duplicate-free at outbox admission as well as in later replay artifacts.
Scan-planned and scan-tasks-fetched replay must also carry a complete
authorization receipt at the LakeCat boundary: valid principal, the
event-matching `table-plan-scan` catalog action, affirmative decision,
non-empty engine, and RFC3339 `checked_at` timestamp before acknowledgement,
graph projection, or OpenLineage projection. Valid-but-wrong actions such as
table load or commit actions must fail before governed scan proof reaches graph
or lineage sinks.
Service replay must also close the top-level `table.scan-planned` and
`table.scan-tasks-fetched` payload schemas over the fields current producers
emit, so an archived governed read cannot append unverified scan, lineage,
graph, QueryGraph, or application claims beside otherwise valid restriction,
projection, stats, filter, task-count, and authorization evidence.
Prefer upstream Sail APIs for any reusable planner or manifest work.

### P2 QGLake Acceptance

Keep the live QGLake handoff harness in the verification loop. QueryGraph must
continue importing LakeCat evidence without losing view receipt-chain hashes,
accepted view versions, graph proof, import proof, or OpenLineage replay
anchors. QueryGraph bootstrap replay must bind table artifacts to
`verified-tables`, and view artifacts plus view-version receipts to
`verified-views`, plus a valid authorization receipt principal and the
`graph-read` action, so saved bootstrap evidence cannot become actorless,
action-drifted, or splice semantic artifacts and receipt chains across
manifests. Those verified table/view manifests must also be duplicate-free at
outbox admission and compact handoff verification, so QueryGraph bootstrap
replay and archived handoffs cannot inflate counts by repeating accepted
stable IDs. The bootstrap manifest verifier must enforce the same
duplicate-free stable-id invariant across table projections, table artifacts,
view projections, and view artifacts before a bundle becomes QGLake import
proof. Service replay must also close `table-artifacts`, `view-artifacts`, and
`view-version-receipts` entries over the fields LakeCat verifies before
acknowledgement, graph projection, OpenLineage projection, or QGLake handoff
proof, so unverified semantic artifact, standards, graph, or view receipt
claims cannot ride beside matched bootstrap identities and hashes. Scan replay
must preserve the
server-derived purpose and
policy-derived TTL cap in both captured LakeCat replay text and compact handoff
proof. Compact governed scan proof must also preserve the planned and fetched
scan receipt identities: principal subject/kind, full authorization receipt
hashes, and the `table-plan-scan` action from source replay through captured
LakeCat replay and archived handoff summary verification.
The compact `governedScanProof` object and captured LakeCat replay `scan`
object must be closed over the fields LakeCat actually compares, so archived
handoffs cannot append unverified scan-planning, restriction, projection,
stats-field, replay-hash, or OpenLineage claims beside the Sail-planned read
proof. Nested planned/fetched read-restriction objects and their row-predicate
children must also be closed over their verified fields, so a handoff cannot
hide unverified purpose, policy, predicate, projection, or credential-scope
claims inside otherwise matched restriction evidence. Credential replay
must preserve the policy-derived TTL cap, full authorization receipt hash, the
`credentials-vend` authorization action, and redacted storage-scope hash in
raw lineage drains, captured LakeCat replay evidence, and compact handoff
summaries. Credential-vend replay should fail when top-level
read-restriction evidence or `lakecat:raw-credential-exception` evidence drifts
from the authorization receipt context, or when returned credential evidence
or nested storage-profile warehouse / top-level storage-profile id /
secret-reference evidence drifts from the catalog-derived storage profile,
principal, governed-read, and TTL fields. Returned credential replay evidence
must also surface redacted `credentialPrefixHashes` and keep them
count-aligned, full SHA-256-shaped, and duplicate-free by `prefix-hash` in both
raw lineage drains and compact handoff summaries, so credential counts cannot
be inflated with repeated redacted credential entries. Service outbox admission
must also close each `credential-response-evidence` entry over the
catalog-derived credential fields it validates, so replay cannot attach
unverified credential-scope, issuer, storage, authorization, or secret-ref
claims beside an otherwise valid redacted prefix proof. Service outbox admission
must also close the top-level and authorization-receipt-context
`lakecat:raw-credential-exception` objects over the raw exception fields
LakeCat actually compares, so replay cannot attach unverified raw-credential
posture claims beside an otherwise matching blocked-agent or trusted-human
exception. Captured LakeCat replay
JSON must then match the compact credential proof for those prefix hashes, so
an archived handoff cannot pair a valid summary with drifted credential replay
output.
Management-list replay for servers, projects, warehouses, policy bindings, and
storage profiles must also preserve nonblank principal subject/kind evidence,
full authorization receipt hashes, and full replay/OpenLineage SHA-256 hash
arrays before compact QGLake proof is built. These are LakeCat/QGLake/TypeSec
control-plane proof requirements around standard Iceberg catalog state, not
custom Iceberg table metadata.

View receipt-chain proof must remain a structural proof, not just a bag of
hashes: the first receipt must be a version-1 upsert without previous-link
fields, and each later link must point at the previous receipt hash, use a
supported operation, and preserve the expected version transition. Service
replay must reject duplicate receipt, drop-receipt, and chain hash arrays
before projection so later compact QGLake proof cannot inherit inflated
view-history evidence. Memory and Turso store reads must also reject corrupt
durable view receipt chains whose previous links no longer match the prior
receipt, so service replay and QueryGraph handoff never start from forged
store-level view-history evidence.
View receipt-list replay must also carry valid warehouse, namespace, view, and
authorization receipt principal evidence plus the read-side `view-load` action
before projection, so receipt hashes cannot become actorless, scope-free, or
mutation-authorized view-history facts.
View receipt-chain replay must likewise carry valid warehouse, namespace,
authorization receipt principal, the read-side `view-load` action, and
count-aligned chain, receipt, and tombstone totals before projection, so
namespace-level view-history evidence cannot inflate chains, shed chains, or
reuse a mutation receipt by drifting the summary counts or action.
Service replay must close the top-level view receipt-list and receipt-chain
payloads over the fields current producers emit, so archived view-history reads
cannot append unverified view-history, lineage, graph, QueryGraph, or
application claims beside otherwise valid warehouse, namespace, receipt-hash,
chain-hash, tombstone, and authorization evidence.
Service replay must close nested receipt-chain and receipt objects over the
fields LakeCat verifies before acknowledgement, graph projection, OpenLineage
projection, or QGLake proof, so unverified view-history, principal, lifecycle,
or graph claims cannot ride beside structural receipt-chain evidence.
Compact `viewReceiptChainProof`, captured replay `views`, and every nested
accepted-view, tombstone, receipt-chain group, structural chain, and receipt
object must be closed over the fields LakeCat verifies. A handoff or captured
replay sidecar cannot attach unverified view lifecycle, tombstone,
receipt-chain, principal, replay, or OpenLineage claims beside checked
structural view proof.
Lineage-drain replay summaries must also stay bound to the drain-level event
type manifest: a compact QGLake handoff cannot include a replay summary for an
event type that the drain did not declare as delivered, and repeated event
types must match replay summary multiplicity rather than only set membership.
The manifest order must also match replay summary order, so `eventTypes` is a
compact replay sequence proof rather than a reorderable inventory.
Accepted lineage-drain artifacts must also reconcile their top-level
`delivered`, `eventTypes`, `graphEvents`, and `lineageEvents` totals with the
actual replay summary array before the handoff can be treated as verified.
Saved `lakecatHandoffVerifyOutput` artifacts must stay bound to the archived
lineage-drain artifact as well, including delivered count, event type manifest,
graph event count, lineage event count, and the drain-read authorization action
from the compact request-identity proof. A saved handoff must carry a full
`lakecatHandoffVerifyOutputHash`; missing, null, or short self-verifier hashes
are rejected instead of treating the self-verifier sidecar as optional once the
artifact path is present. Compact request-identity and
QueryGraph-bootstrap proofs must also preserve their expected authorization
actions directly: `requestIdentityProof` is a `lineage-read` proof for the
drain read, while `queryGraphBootstrapProof` is a `graph-read` proof for the
bootstrap event. Saved `lakecatHandoffVerifyOutput` sidecars must reject
top-level copies of those compact proofs when either authorization action
drifts. Compact `requestIdentityProof` and captured LakeCat replay
`requestIdentity` proof objects must also stay closed over the fields LakeCat
compares, so a summary, captured replay output, or saved self-verifier sidecar
cannot attach unverified actor, identity-source, TypeDID, authorization, or
drain-read action claims beside the accepted request-identity evidence.
Compact `queryGraphBootstrapProof` and captured LakeCat replay
`queryGraphBootstrap` proof objects must likewise stay closed over the fields
LakeCat compares, so a summary, captured replay output, or saved
self-verifier sidecar cannot attach unverified bundle/import, artifact-count,
standards, identity, TypeDID, authorization, delegation, view-receipt, replay,
or OpenLineage claims beside the accepted QueryGraph bootstrap proof.
Compact table commit-history proof must preserve the same explicit
zero-count and duplicate-free commit-hash invariants as service replay, so
archived QueryGraph handoff summaries can represent an empty history without
fabricating commits and cannot inflate pointer-log evidence by repeating a
valid commit hash. Compact `tableCommitHistoryProof` and captured LakeCat
replay `tableCommitHistory` proof objects must also stay closed over the
fields LakeCat compares, so a summary, captured replay output, or saved
self-verifier sidecar cannot attach unverified pointer-log claims beside the
accepted count, sequence, hash, principal, authorization, graph, replay, and
OpenLineage evidence.
Compact catalog-config proof must also preserve the same advertised defaults,
overrides, endpoints, `catalog-config` authorization action, graph count,
replay hashes, and OpenLineage hashes as raw `catalog.config-read` replay, so
archived QGLake summaries and captured replay sidecars cannot drop the v4 bridge
posture or integration discovery contract after source replay accepted it.
Compact `catalogConfigProof` and captured LakeCat replay `catalogConfig` proof
objects must also stay closed over those compared fields, so a summary,
captured replay output, or saved self-verifier sidecar cannot attach unverified
v4 bridge, endpoint, authorization, graph, replay, or OpenLineage compatibility
claims beside checked config-read proof.
Saved `lakecatHandoffVerifyOutput` sidecars must bind their own
`lineageDrainArtifactSemantics.catalogConfigProof` to the raw lineage-drain
artifact too, so a self-verification artifact cannot claim verified drain
semantics while omitting or rewriting the config-read compatibility proof.
Saved `lakecatHandoffVerifyOutput.artifactFiles` must also use full SHA-256
digests for its nested bundle, lineage-drain, QueryGraph import-plan, captured
LakeCat/QueryGraph output, and service-log hashes before those values are
compared with the compact handoff summary. Equality to the summary is not
enough if the saved self-verifier artifact can carry placeholder or prefix-only
hashes. The same object must be closed over the known artifact manifest: extra
top-level artifact keys or extra nested captured-output keys are rejected so a
saved sidecar cannot smuggle unverified artifact claims alongside the checked
bundle. Each nested artifact and captured-output hash object must also be
closed over `sha256` only, so a sidecar cannot attach alternate hash claims to
an otherwise accepted artifact hash. The saved self-verifier root and
`capturedOutputSemantics` object must also be closed over their known schema
keys, so a sidecar cannot append unverified proof sections or captured-output
semantics that no verifier compares to the compact summary. Individual saved
semantic proof sections must be closed as well: LakeCat replay semantics,
QueryGraph verify/import semantics, bundle artifact semantics, import-plan
semantics, and lineage-drain semantics may carry only the fields the verifier
compares.
Handoff artifact paths must resolve under the handoff summary directory before
LakeCat hashes or parses them. A saved summary must not be able to splice in an
absolute path or `..` traversal to matching bytes outside the archived bundle.
The same bundle-local resolver must be used by semantic artifact readers as
well as hash verification, so captured output, bundle, import-plan, and
lineage-drain parsers cannot drift from the public handoff verifier.
Raw lineage-drain replay summaries and compact handoff summaries must both keep
replay, OpenLineage, commit-history commit, view receipt, and view
receipt-chain hash arrays duplicate-free as well as full SHA-256-shaped, so
source replay and archived proof cannot inflate bootstrap, scan, management,
commit-history, view, storage-profile, or credential evidence by repeating a
valid digest. Service
drain should reject projection receipts whose replay/OpenLineage hash arrays
are count-drifted, malformed, or duplicate before returning a raw
lineage-drain summary or acknowledging delivery.
Compact QGLake storage-profile and credential secret-reference proof must
mirror service replay admission: present secret refs require nonblank providers
and full SHA-256 hashes, while absent secret refs may omit provider/hash fields
or encode them as null, but any other provider/hash value is rejected.
Compact management proof must preserve the same duplicate-free ID invariant as
service replay, so saved QGLake summaries and lineage-drain artifacts cannot
inflate server, project, warehouse, policy, or storage-profile reads by
repeating valid control-plane identities. It must also preserve warehouse-list
project scope as compact `warehouseProjectId` evidence and reject malformed or
unlisted scopes, so archived QGLake management proof cannot detach a
project-filtered warehouse inventory from the project list it claims.
Service replay admission must also close the top-level payload schema for
`namespace.listed`, `view.listed`, and management list events, so an archived
inventory read cannot append unverified namespace, view, management,
OpenLineage, replay, or QueryGraph claims beside otherwise valid count and
ID/name/path evidence before acknowledgement or projection.
Policy-list proof must be paired with policy-upsert content proof: compact
`managementProof.policyUpsertProof` must carry a policy id listed in
`policyIds`, a full ODRL content hash, principal subject/kind, full
authorization receipt hash, `policy-manage` action proof, graph event proof,
replay hashes, and OpenLineage hashes, and raw lineage replay must reject
missing or malformed `policy-binding.upserted` evidence before
QueryGraph/QGLake handoff is accepted.
Compact `managementProof`, captured replay `management`, and nested
`policyUpsertProof` must also be closed over their compared fields. Captured
`warehouseProjectId` must match compact scope evidence, while captured-only
`storageProfileUpsert` remains verified by the sibling storage-profile proof.

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
carry the accepted replay principal subject/kind and an explicit commit count.
An empty history is valid zero-count proof and must drain without fabricating
commit graph nodes; present commit entries must carry positive, strictly
increasing sequence numbers and duplicate-free commit hashes at the service
outbox boundary, raw lineage-drain boundary, and compact handoff proof, so
pointer-log evidence cannot drop or rewrite actor attribution, duplicate
commits, or reorder between the catalog and QGLake proof. Service replay
admission must require a valid authorization receipt principal for every
`table.commits-listed` source event, bind `principal-subject` and
`principal-kind` to that receipt, and bind warehouse/namespace/table evidence
to the durable outbox table identity, so graph and OpenLineage projection
never observe actorless or cross-table pointer-log reads. Raw lineage-drain
and compact handoff proof must also preserve a full authorization receipt hash
and the read-side `table-load` action for `table.commits-listed`; regressions
continue to cover missing and drifted commit-history principal subject,
principal kind, and action proof before compact handoff proof generation, and
service replay admission must reject a valid mutation action such as
`table-commit` on a commit-history read before graph or OpenLineage projection.
Service replay must also close the top-level `table.commits-listed` payload
schema over the fields current producers emit, so archived commit-history reads
cannot append unverified commit, pointer, lineage, graph, QueryGraph, or
application claims beside otherwise valid table scope, count, sequence, commit
hash, principal, and authorization evidence.
Pending outbox replay should stay deterministic across embedded and Turso
stores, ordered by `created_at,event_id`, with batch limits applied after that
order and with duplicate-safe delivery accounting. Draining should acknowledge
delivery only after every projection in the batch succeeds, leaving events
pending for retry when graph or lineage projection fails.
Store-level commit idempotency evidence must also be shaped before a durable
mutation starts, not only at the REST header boundary: blank or malformed keys,
caller-provided request hashes without keys, and non-SHA-256 request hashes
must fail before pointer movement, pointer-log insertion, audit, outbox
emission, or idempotency replay.
Individual `table.commit` replay evidence must carry a positive sequence
number before acknowledgement or projection, matching the positive,
strictly-increasing invariant used by commit-history replay.
It must also carry non-empty new metadata pointer evidence, and any previous
metadata pointer evidence must be non-empty when present, before
acknowledgement or projection. The replay evidence must include both a valid
commit principal and a valid authorization receipt principal, and those
principals must match before graph or OpenLineage projection, so replay cannot
drop or rewrite the actor associated with a committed pointer transition.
The commit replay envelope must also include full SHA-256 request and response
hash evidence before projection; an idempotency-key hash is optional for
standard Iceberg commits that did not supply LakeCat's retry key, but must be
full SHA-256 evidence whenever present. The policy hash remains optional when
no policy participated. It must also carry
positive Iceberg format-version evidence and non-negative snapshot-id evidence,
so graph and OpenLineage projections cannot lose the table-format summary that
the pointer-log path exposes later. Service replay admission must also close
the nested `commit` object over the pointer-transition, identity, authorization,
hash, format, snapshot, and timestamp fields LakeCat actually verifies, so a
durable `table.commit` event cannot append unverified commit, policy, storage,
or graph claims beside an otherwise valid pointer transition. The store
producer now rejects table and
commit metadata that lacks positive `format-version` evidence before producing
durable commit records, and it emits explicit `snapshot_id: 0` evidence for
commits where the Iceberg metadata has no current snapshot, so a schema-only or
empty-table commit does not create an undrainable `table.commit` event. The
individual commit envelope must also carry an RFC3339 committed-at timestamp
before acknowledgement or projection, so replay cannot preserve pointer
movement while dropping the time at which the catalog accepted it.

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
Secret-manager payload parsing must also fail closed on malformed credential
config shapes, including blank config keys, before any secret-backed
credential response is issued. Configured cloud-style provider backend
failures must also stay hash-only: provider label, `secret-ref-hash`, and
`error-detail-hash` are admissible diagnostics; raw secret refs, account paths,
tokens, ARNs, backend exception text, or secret names are not.
AWS/GCP/Azure-style `aws-sm://`, `gcp-sm://`, and `azure-kv://` references may
use file-backed provider roots for local and single-node deployments. These
roots are configured with provider-specific environment variables, use the
SHA-256 digest of the secret reference as the JSON filename, and still dispatch
only after TypeSec authorizes the exact secret-ref resource. They are a
redacted built-in backend, not a claim of cloud SDK integration.
Credential responses should carry catalog-derived secret-ref provider and
secret-ref hash evidence when a storage profile uses an external secret
reference, and backend-supplied provider/hash evidence must be replaced rather
than trusted. Credential-vend replay must also reject response evidence whose
secret-ref provider or hash drifts from the selected storage profile before any
graph or OpenLineage sink observes it.
It must bind the replay payload table hint to the durable outbox table identity
before projection, so a credential-vend event cannot project one table's
credential-root decision as another table's graph or lineage evidence.
Credential-vend replay must validate the nested storage-profile
provider/issuance-mode and secret-ref/mode proof even when no credentials were
returned, so blocked attempts cannot project a weaker credential-root claim
than storage-profile management would accept. The replay payload must also
carry top-level `secret-ref-present` evidence that matches the nested storage
profile, so compact credential proof cannot omit or contradict whether the
selected credential root depends on an external secret reference.
Replay admission for both `storage-profile.upserted` and
`credentials.vend-attempted` must also re-check nested storage-profile
`public-config` objects: values must stay string-shaped, secret-like keys or
values must fail with hash-only public-config-key evidence, and LakeCat-reserved
credential evidence keys must be rejected before graph, OpenLineage, or QGLake
proof can treat those public hints as accepted credential-root facts.
The compact `credentialVendingProof` object and captured LakeCat replay
`credentials` object must stay closed over their compared field set at the
top level, branch level, and nested redacted storage-profile level. Archived
handoffs must reject unverified raw credential, storage-scope, authorization,
replay, or OpenLineage claims before QueryGraph indexes them as accepted
TypeSec-style credential decisions.
Storage-profile upsert replay must be hash-only for storage roots: generated
audit/outbox evidence records `location-prefix-hash`, and raw
`location-prefix` values must fail before acknowledgement or projection.
Storage-profile replay must also carry unambiguous credential-root identity:
non-empty profile id, valid nested warehouse matching any top-level warehouse,
valid provider, and valid issuance mode.
Service outbox admission must close the nested `storage-profile` object over
the redacted producer schema for both `storage-profile.upserted` and
`credentials.vend-attempted` replay. Unexpected nested storage-profile fields
must fail before acknowledgement, graph projection, OpenLineage projection, or
QGLake proof can inherit unverified credential-root or storage-scope claims.
Management-upsert replay for policy bindings, projects, servers, storage
profiles, and warehouses must also carry a valid authorization receipt
principal plus an event-matching catalog action, affirmative allowed decision,
non-empty engine, and RFC3339 `checked_at` timestamp before projection, so
tenant-root and policy mutations cannot become actorless or action-drifted
catalog graph or OpenLineage facts.
Service outbox admission must close nested project, server, and warehouse
record objects over their route-produced fields, so replay rejects unexpected
tenant-root, endpoint, or storage-root claims before acknowledgement, graph
projection, OpenLineage projection, or QGLake proof.
Server and warehouse upsert replay must also bind redaction hashes back to the
source value when that value is present: `endpoint-url-hash` must recompute
from `endpoint-url`, and `storage-root-hash` must recompute from
`storage-root`, before graph, OpenLineage, or QGLake proof accepts the
management event.
Storage-profile upsert replay and compact QGLake handoff proof must also bind
that principal to a full authorization receipt hash and the
`storage-profile-manage` action, beside the redacted provider, issuance mode,
location-prefix hash, optional secret-reference hash, graph event count, replay
hashes, and OpenLineage hashes. This keeps credential-root management proof from
being replayed as a weaker table or lineage action.
The compact `storageProfileUpsertProof` object and captured LakeCat replay
`management.storageProfileUpsert` object must be closed over those compared
fields, so archived handoffs cannot append unverified credential-root,
provider, secret-reference, authorization, graph, replay, or OpenLineage
claims beside checked storage-profile management proof.
Policy-binding upsert replay must also bind captured ODRL material to a full
`odrl-hash` before graph or OpenLineage projection. LakeCat validates the
catalog scope and content anchor, while TypeSec and QueryGraph remain the
places for policy interpretation and semantic composition.
Service outbox admission must also close the nested policy-binding `policy`
object over the route-produced fields, so replay rejects unexpected ODRL,
governance, scope, or enforcement claims before acknowledgement, graph
projection, OpenLineage projection, or QGLake proof.
Server and warehouse upsert replay must also treat endpoint URLs and storage
roots as sensitive management roots. Generated audit/outbox evidence should
persist `endpoint-url-hash` and `storage-root-hash` instead of raw roots, and
legacy durable events that still carry raw `endpoint-url` or `storage-root`
values must include the matching full SHA-256 hash evidence before any graph or
OpenLineage projection.
Provider and issuance-mode compatibility must be replay-checked too:
`local-file-no-secret` requires the file provider, while
`short-lived-secret-ref` requires a cloud object provider.
Secret-reference presence must match issuance mode: short-lived secret-ref
profiles require redacted secret-ref proof, while governed-read and no-secret
profiles cannot carry secret-reference proof. Secret-ref providers must be
nonblank whenever proof is required, and provider/hash fields must be absent
when `secret-ref-present` is false, regardless of JSON type.
Blocked raw-credential replay must carry zero credentials plus a non-empty
block reason matching the raw-credential exception receipt context before any
graph or OpenLineage sink observes it.
Credential-vend replay must also carry a valid authorization receipt
principal, full authorization receipt hash, and the event-matching
`credentials-vend` action before projection; valid-but-wrong actions such as
read or commit actions must fail before acknowledgement, graph projection, or
OpenLineage projection. This applies even to blocked zero-credential attempts
where no returned credential response entry exists to repeat actor evidence.
Management-list replay must carry count-aligned, syntactically valid,
duplicate-free ID arrays plus a valid authorization receipt principal,
event-matching catalog action, affirmative allowed decision, non-empty engine,
and RFC3339 `checked_at` timestamp before projection, so compact QueryGraph
proof cannot inflate server, project, warehouse, policy, or storage-profile
reads with repeated, actorless, or action-drifted identities. Warehouse-list
replay must also reject blank or syntactically invalid `project-id` scope when
that optional project filter is present, so project-scoped warehouse inventory
cannot become malformed QueryGraph or OpenLineage proof.
Standard catalog replay for catalog config reads, namespace list/lifecycle
events, and view list/lifecycle events must carry valid authorization receipt
principals before projection too, so Iceberg-compatible control-plane evidence
cannot become actorless graph or OpenLineage facts. Namespace replay must also
preserve event-matching actions: `namespace.listed` uses `namespace-list`,
`namespace.created` uses `namespace-create`, `namespace.loaded` uses
`namespace-load`, and `namespace.dropped` uses `namespace-drop`. View-list
replay must use the read-side `view-load` action; `view-manage` is reserved for
view mutations, so service replay and QGLake handoff action contracts stay
aligned. View lifecycle replay must also preserve event-matching actions:
`view.upserted` uses `view-manage`, `view.loaded` uses `view-load`, and
`view.dropped` uses `view-drop` before graph or OpenLineage projection.
Namespace lifecycle replay must also close the top-level payload schema over
`event-type`, `authorization-receipt`, `warehouse`, and `namespace`, so
create/load/drop replay cannot append unverified namespace, scope, graph,
lineage, or QGLake claims beside otherwise valid standard catalog evidence.
Table lifecycle replay for create, load, delete, and restore events must carry
the same valid authorization receipt principal plus an event-matching catalog
action, affirmative allowed decision, non-empty engine, and RFC3339 `checked_at`
timestamp before projection, so table lifecycle graph/OpenLineage facts cannot
be actorless or action-drifted even when the standard Iceberg REST response
shape remains unchanged.
Create, load, and restore replay must also carry both the unsigned table
version that current producers emit and positive Iceberg `format-version`
evidence. Delete replay carries the same pointer-generation and table-format
evidence through required `soft-delete.version` and `soft-delete.format_version`
or `soft-delete.format-version`, and `table.deleted` replay must reject missing
soft-delete objects, non-positive soft-delete versions, or missing/non-positive
soft-delete format versions before acknowledgement, graph projection, or
OpenLineage projection. Full table identity objects and soft-delete objects are
also closed over the fields LakeCat verifies before projection, so unverified
table-scope, delete-state, principal, storage, or application claims cannot ride
beside the checked lifecycle identity and soft-delete proof.

View lifecycle replay must carry valid view names and positive store-assigned
`view-version` values before projection, and guarded view lifecycle replay must
reject non-positive `expected-view-version` values, so QueryGraph receipt chains
cannot be extended by invalid or versionless guarded requests. Service replay
also closes the nested `view` object over the route-produced view schema before
acknowledgement, graph projection, OpenLineage projection, or QGLake proof, so
unverified view lifecycle claims cannot ride beside the validated view identity,
SQL dialect/schema, columns, properties, and store-assigned version.

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
Default and override entries are closed over `key` and `value` before
acknowledgement, graph projection, OpenLineage projection, or QGLake config
proof, so unverified compatibility, v4, or integration-discovery claims cannot
ride beside the checked catalog config contract.
Until typed Sail v4 support is available, `lakecat.format.v4*` defaults are a
pinned claim namespace: replay must reject unsupported extra v4 bridge keys
rather than letting future-looking typed-Sail claims coexist with
`typed-sail=unavailable`. Config overrides cannot carry v4 bridge keys either;
until Sail exposes stable typed v4 support, an override is not allowed to
rewrite the catalog's advertised v4 posture.
Catalog config-read replay must also bind the advertised endpoint list to the
standard Iceberg REST surface: config, namespace list/create, table create,
table load, and table commit endpoints must be present for both default and
warehouse-prefixed routes before graph or OpenLineage projection can treat the
config read as compatibility evidence. The same replay evidence must preserve
LakeCat's governed access surfaces for both route forms: plan, fetch-scan-tasks,
and credential endpoints are additive proof-carrying catalog APIs over standard
tables, not required custom Iceberg metadata or QueryGraph-only routes.
Config replay must also preserve LakeCat's integration discovery surfaces:
`POST /management/v1/lineage/drain` and `GET /querygraph/v1/bootstrap`. These
are not standard Iceberg REST table-access requirements; they are additive
LakeCat/QueryGraph/OpenLineage control-plane surfaces that let QGLake imports
and lineage drains prove which integration contract was advertised when the
config read entered graph or lineage projection.
Compact QGLake handoff proof must carry that config-read contract forward as
`catalogConfigProof`, and captured LakeCat replay sidecars must match it
exactly, so saved handoffs cannot accept weaker v4, endpoint, or
integration-discovery evidence than the raw lineage drain proved.
Saved handoff verifier output must also repeat that same proof under
`lineageDrainArtifactSemantics.catalogConfigProof`, binding the verifier's
claim about the raw drain artifact to the config-read evidence it actually
validated.
The local dependency contract is the guardrail while cloud CI is manual-only:
it must reject automatic triggers across every GitHub workflow file, not just
the primary CI workflow, including compact, block-list, inline-map, and quoted
YAML trigger forms, before any pushed slice depends on cloud feedback. The
guard should inspect actual `on:` trigger declarations while allowing harmless
workflow keys such as job ids that happen to share event names.
The first-release gate is local too:
`scripts/check-release-readiness.sh` is the durable release checklist command,
with full mode running dependency-contract, formatting, Turso, Sail, TypeSec,
Grust, explicit all-features CLI, complete all-features workspace, book, and
QGLake handoff checks, and quick mode available for script/contract smoke
checks during narrow slices. The default workspace run still covers ordinary
doc-tests; feature-matrix rows target package unit tests so an empty rustdoc
phase cannot hang after the relevant Turso/Sail/TypeSec/Grust tests have
already passed. The complete all-features workspace row remains the final broad
local proof before release or cloud automation is trusted.

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
- Before cutting a first release, run `scripts/check-release-readiness.sh`
  locally and treat it as the authoritative gate while cloud CI is manual-only.
- Commit each logical unit only after the corresponding docs/status are
  reconciled.
