# LakeCat Review (OPUS2)

Second full review of `~/src/lakecat` (branch `master`, HEAD `293b71d`), against
the new [`GOAL.md`](../GOAL.md) and the integration targets `~/src/sail`,
`~/src/grust`, `~/src/typesec`, and `~/src/querygraph`.

[OPUS1.md](OPUS1.md) reviewed a scaffold with *no commits* — a faithful
Milestone-1 skeleton that was "not yet a catalog." This review covers what the
repo has become after ~80 commits: it grades the **current** code, records which
OPUS1 findings closed, and re-aims the remaining work at the GOAL. The companion
[OPUS2-DESIGN.md](OPUS2-DESIGN.md) carries the design-level plan forward.

The headline has changed. OPUS1: *"the seams are right, but it is not yet a
catalog."* OPUS2: **it is now a catalog — an authenticated, durably-committing,
CAS-correct, governance-gated Iceberg REST catalog with an in-process Sail
provider and a CLI.** The frontier has moved up one layer: from *"is it a
catalog?"* to *"is it a* governed *catalog?"* — and there the answer is *"the
gate is real, but the governed read path does not yet narrow data."*

---

## Verification notes

All commands run in this environment against `293b71d`.

| Command | Result |
| --- | --- |
| `cargo test --workspace` (default features) | **PASS** (exit 0) |
| `cargo test --workspace --all-features` | **PASS** — 66 tests, 0 failed |
| `cargo fmt --all -- --check` | **PASS** (only the documented nightly-only `imports_granularity` / `group_imports` warnings) |
| `git diff --check` | **PASS** |

`--all-features` turns on `sail-local`, `typesec-local`, `grust-local`, and
`turso-local` together and stays green, and — the OPUS1 review-gate discipline —
the **default** feature set is green too. The C++/`cxx` toolchain hazard that
affects the Grust workspace does not reach LakeCat: nothing on its dependency
path compiles C++.

Codebase size: **~15,548 Rust lines across 10 crates** (OPUS1: ~6,102 across 9).
The growth is concentrated where OPUS1 said the catalog was missing —
`lakecat-store` (254 → 2,783), `lakecat-service` (1,100 → 4,186),
`lakecat-sail` (3,259 → 5,087), `lakecat-security` (200 → 743) — plus the
previously-absent `lakecat-cli` (899).

---

## Executive summary

The load-bearing spine OPUS1 said was missing is now present and, where I read
it closely, **correctly built**:

- **Authentication & typed principals.** Every handler resolves a `Principal`
  from `x-lakecat-principal[-kind]`, `x-lakecat-agent-did`, `x-lakecat-typedid`,
  or a `Bearer` token (hashed, never stored raw), falling back to anonymous only
  when nothing is supplied. A sanitized `lakecat.request-identity.v1` envelope —
  proofs reduced to SHA-256 — rides into the governance context.
- **The capability *is* the proof.** `Capability<Action, Resource>`
  ([lakecat-security/src/lib.rs:55](../crates/lakecat-security/src/lib.rs)) has
  private fields and is mintable only via `from_receipt`, which rejects a receipt
  unless `allowed` is true, the action matches, and the table scope matches.
  Every privileged handler takes the *typed* capability, not a boolean — an
  unauthorized path cannot construct one. This is exactly the model OPUS1-DESIGN
  argued for.
- **Durable, CAS-correct, auditable commit.** The Turso `commit_table`
  ([lakecat-store/src/lib.rs:1144](../crates/lakecat-store/src/lib.rs)) runs one
  transaction that: replays idempotency records, re-checks the expected previous
  pointer, performs the pointer swap as a guarded `UPDATE … WHERE
  metadata_location = :prev` (treating `rows_affected == 0` as a conflict), and
  writes `metadata_pointer_log` + `audit_events` + `outbox_events` (+ optional
  `idempotency_records`) before commit. A concurrent-writer regression proves one
  writer wins and one gets a conflict.
- **Transactional outbox, drained off the request path.** Graph/lineage side
  effects are no longer inline `emit().await?`; committed events land in
  `outbox_events` and `drain_outbox_once` projects them to the graph and lineage
  sinks. A flaky sink can no longer fail a commit or surface a rolled-back one.
- **Sail promoted toward Tier 1.** `LakeCatCatalogProvider`
  ([lakecat-sail/src/lib.rs:340](../crates/lakecat-sail/src/lib.rs)) implements
  Sail's real `CatalogProvider` trait and runs the LakeCat governance gate
  *inside* each method, minting the same typed capabilities — the "policy and
  plan fuse in one process" property the design called the architectural prize.
- **Real engines are wirable, and there's a CLI.** `lakecat-service` exposes
  `sail-local` / `typesec-local` / `grust-local` passthroughs; `lakecat-cli`
  (absent in OPUS1) drives config, storage-profile and policy management, bundle
  export, and a QGLake fixture.

What remains is no longer *plumbing* — it is the **governed-data frontier**: the
read path authorizes but does not yet *mask*, and ODRL bindings are transported
but not *interpreted*. Until those land, the QGLake "metadata-visible,
data-denied" broker — the GOAL's acceptance proof — cannot be demonstrated, even
though every seam it needs now exists.

---

## OPUS1 findings — status at `293b71d`

| # | OPUS1 finding | Now |
| --- | --- | --- |
| 1 | Red default-feature tests | **CLOSED** — default + all-features green; Sail-specific assertions gated |
| 2 | No auth / real principal | **LARGELY CLOSED** — typed principal resolution, capability model, persisted receipts; TypeDID verification has a `typesec-local` seam. Remaining: production TypeDID resolver/keys (see F2 below for the masking successor) |
| 3 | No durable / CAS commit | **CLOSED (Turso spine)** — real single-txn CAS, pointer log, audit, outbox; idempotency *store-side only* (see F3) |
| 4 | No persistence backend | **CLOSED (Turso spine)** — namespaces, tables, pointer log, idempotency, audit, outbox, storage profiles, policy bindings, soft-delete |
| 5 | Service can't activate real engines | **CLOSED** — `sail-local`/`typesec-local`/`grust-local` passthroughs + CLI wiring |
| 6 | Sail used as struct library | **PARTIAL** — in-process `CatalogProvider` (Tier 1) for catalog ops; scans still walk manifests in-process (see F5) |
| 7 | Plan ↔ impl drift; no CLI | **PARTIAL** — CLI landed, `CatalogAction` expanded, stable IDs aligned; Server/Project/Warehouse plus semantic views are durable management entities; standard Iceberg view REST and richer hierarchy routing remain (see F7) |
| 8 | Grust graph placeholder | **PARTIAL** — taxonomy + ingestion moved into Grust; emission is still event→table breadcrumbs (see F6) |
| 9 | `list_namespaces` fabricates default | **CLOSED** — memory + Turso return empty |
| 10 | Side effects coupled to request | **CLOSED** — transactional outbox + drain |
| 11 | Plan tokens unauthenticated | **CLOSED** — structured, table-bound, HMAC-signed, path-revalidated |
| 12 | v4 = JSON passthrough | **OPEN (by design)** — behind capability flag |

Nine of twelve are closed or largely closed. The three "partial" rows are the
spine of the work ahead, restated below in current terms.

---

## Findings (current state, ordered by severity)

### F1. (HIGH) The governed read path gates access but does not *mask* data

**Files:** `crates/lakecat-service/src/lib.rs::plan_scan_with_capability`
(line ~1312); `crates/lakecat-sail/src/lib.rs` scan planning.

`plan_table_scan` mints a `TableScanCapability` (good — no plan without
authorization) and then plans using the **client-supplied** projection and
filters. There is no policy-derived narrowing: a grep for column/row masking in
the scan path returns nothing. So an authorized agent receives exactly what it
asked for, not "strictly less than a human."

This is the single most important gap relative to `GOAL.md`, which makes
"governed Sail-planned reads the default for agents" the core promise, and to
OPUS1-DESIGN's central thesis that *the masked/filtered plan must be the only
plan that exists.* The capability gate answers **whether** a principal may scan;
it does not yet constrain **what** the scan returns. The QGLake "metadata-visible,
data-denied" `RestrictedDataBroker` cannot be built on a gate alone.

**Fix:** derive a column/row restriction from the principal's policy bindings
(F2) and pass it into the Sail planning request as a mandatory, server-owned
projection/filter intersection — never a client input. The capability should
carry the *effective* allowed columns/predicate so the audit receipt records what
was enforced, and `fetch-scan-tasks` must re-apply the same restriction (a token
must not let a client widen its projection). This is the work that makes the
in-process provider (F5) worth finishing: fused policy+plan is only valuable once
the plan is actually narrowed.

### F2. (HIGH) ODRL bindings are transported but not interpreted; `Delegate` collapses to deny

**Files:** `crates/lakecat-service/src/lib.rs::authorize` (line ~1746);
`crates/lakecat-security/src/lib.rs`.

Policy bindings are loaded for the table and attached to the authorization
*context* as opaque JSON, but nothing interprets ODRL permissions/prohibitions/
duties. `TypeSecGovernanceEngine` maps only `PolicyResult::Allow` to
`allowed = true`; `Deny` **and `Delegate`** both become "not allowed" (and in the
credential issuer, `Delegate` is an error). So:

- An `enforced` ODRL binding is effectively advisory — TypeSec sees the action
  and resource, but the usage constraints (purpose, column scope, expiry) never
  shape the decision or the plan.
- The ODRL→RBAC *delegation* composition OPUS1-DESIGN specified does not exist;
  `Delegate` is a dead branch that fails closed instead of consulting RBAC.

The design correctly assigns the *enforceable subset* of ODRL to LakeCat as a
gate primitive. Right now that subset is empty.

**Fix:** (a) compose engines — on `Delegate`, consult the RBAC engine rather than
denying; (b) translate the enforceable ODRL subset (allowed columns, row
predicate, purpose, max credential duration) into the restriction that F1 feeds
to the planner; (c) fold the binding's `odrl` hash into `policy_hash` so the
receipt is bound to the exact policy evaluated.

### F3. (MEDIUM) Commit idempotency is unreachable from the REST path

**File:** `crates/lakecat-service/src/lib.rs::commit_table` (line ~1208).

The store implements full idempotency — replay records keyed by
`{table}:{idempotency_key}`, returned verbatim on retry — but the handler
hardcodes `idempotency_key: None`. The Iceberg REST commit never extracts an
idempotency key, so the documented "idempotent commit" contract is dead over
HTTP. A client that retries a commit after a dropped response re-runs the full
CAS and either double-applies (if the pointer matches) or gets a spurious
conflict.

**Fix:** extract an idempotency key from a request header (e.g.
`x-lakecat-idempotency-key`) or a stable request hash, thread it into
`TableCommit`, and add a service-level retry-replay test. The store machinery is
already there; this is a one-field wiring plus a header contract.

### F4. (MEDIUM) Metadata object is written before the CAS, with no orphan handling or retry

**File:** `crates/lakecat-service/src/lib.rs` — `write_planned_metadata` (line
~1226) runs *before* `store.commit_table` (line ~1198).

The new `…/metadata/NNNNN.json` is written, then the pointer CAS runs. On a
conflict (concurrent committer) the metadata object is already on disk and is
never cleaned up, and there is no retry loop — the request just 409s. Iceberg
tolerates orphan metadata, but under contention this leaks files indefinitely and
pushes location regeneration onto the client. `write_planned_metadata` is also
`file://`-only (documented), so any non-local warehouse commit fails at this
step.

**Fix:** either (a) write to a content-addressed/temp location and only finalize
after the CAS wins, or (b) add a bounded retry that re-reads the pointer,
re-plans, and rewrites under a fresh sequence number, plus a best-effort orphan
delete on terminal conflict. Generalize the writer to the `object_store`
backends already declared in `Cargo.toml`.

### F5. (MEDIUM) Scans still bypass the in-process provider (Sail is the optimizer for commits, not reads)

**Files:** `crates/lakecat-sail/src/lib.rs` (`sail_integration` engine vs.
`LakeCatCatalogProvider`).

Tier 1 is real for catalog operations: the provider fuses governance and
delegates create/load/drop/commit/commit-discovery to the store. But REST
`plan`/`fetch-scan-tasks` still call the `SailCatalogEngine` struct-library path
that walks manifest lists and prunes file bounds *inside LakeCat*, not a
DataFusion plan resolved *through* the provider. So "Sail remains the optimizer"
holds for commit discovery and table status, not for the read plan — and F1's
masking has nowhere natural to live until reads flow through the provider/planner.

**Fix:** route `plan_table_scan` through a Sail logical plan over the
`LakeCatCatalogProvider` (start "free" via Sail's `IcebergRestCatalogProvider`
over REST for correctness, then in-process), with the F1/F2 restriction applied
as the plan's mandatory projection/filter. Keep the current in-process pruning as
the Tier-0 fallback only where a full plan isn't yet wired.

### F6. (MEDIUM) Catalog graph is still event breadcrumbs, not the semantic graph

**File:** `crates/lakecat-graph/src/lib.rs`; emission via
`outbox_table_projection` in the service.

The node taxonomy enumerates `Snapshot/Manifest/Column/Policy/Principal/
ScanPlan/Commit/LineageRun`, and the reusable projection now correctly lives in
Grust (good boundary work). But the only thing ever emitted is
`GraphEvent::table(...)` → a 4-node/3-edge event-to-table fragment. None of the
*semantic* structure the architecture promises is built: no `Namespace CONTAINS
Table`, `Table HAS_COLUMN Column`, `Table CURRENT_SNAPSHOT Snapshot`, `Table
GOVERNED_BY Policy`, or `Principal CAN_PLAN/CAN_COMMIT Table`. QueryGraph still
receives breadcrumbs, not a queryable catalog graph.

**Fix (LakeCat side, bounded):** translate each committed event into the typed
node/edge upserts for the *stable* semantic entities (Namespace/Table/Column/
Snapshot/Policy/Principal/ScanPlan/Commit), keeping file-granularity
(DataFile/DeleteFile/Manifest) out of the graph as metadata-as-data — exactly the
cardinality discipline OPUS1-DESIGN argued. The graph mechanics stay in Grust;
LakeCat owns only the catalog-domain event→typed-graph mapping.

### F7. (LOW) Tenancy hierarchy is durable but not fully routed

**File:** `crates/lakecat-service/src/main.rs`; `management_warehouse` (line
~1613) only checks equality with the one configured warehouse.

Storage profiles, policy bindings, servers, projects, warehouses, and semantic
views are now first-class durable management entities. LakeCat can resolve
warehouse prefixes from stored `WarehouseRecord` values instead of only trusting
the configured default. Governed server list/upsert endpoints persist
`ServerRecord` values in memory and Turso with audited `server.*` events;
durable project records can attach to stored servers; and durable warehouse
records must attach to stored projects. Project-scoped management routes can now
list and upsert those warehouses for QueryGraph/bootstrap callers without
changing standard table access. Governed view list/upsert endpoints persist
`ViewRecord` values in memory and Turso. Warehouse-prefixed catalog REST aliases
now list, load, and upsert those durable views while preserving standard table
access semantics. QueryGraph bootstrap now exports those views with
manifest-covered OSI hashes, view-aware graph edges, and OpenLineage view counts.
The remaining tenancy gap is narrower but real: full typed Iceberg view metadata
and view commit semantics are not fully modeled yet.

**Fix:** converge the durable View records with Sail-backed typed Iceberg view
metadata and commit semantics when upstream model support is ready.

### F8. (LOW) Production secret-store backends fail closed but are unexercised

**File:** `crates/lakecat-service/src/lib.rs::typesec_credential_issuer`.

The TypeSec-gated issuer is well-shaped: `typesec://env/VAR` resolves locally,
`vault://` resolves over HTTP after authorization, and `aws-sm://` / `gcp-sm://` /
`azure-kv://` fail closed with explicit not-configured errors. Good posture. But
Vault is HTTP-only with a mock-backed test, and the cloud resolvers are
unimplemented — the GOAL lists production credential issuance as a milestone, and
raw vending must stay the audited exception, so this needs real backends before
any non-local deployment.

### F9. (LOW, by design) v4 remains JSON passthrough

Unchanged from OPUS1 Finding 12: format v4 bypasses typed `TableMetadata`,
behind a capability flag, with thin coverage. Correct hedge while the spec moves;
converge on typed v4 once `sail-iceberg` gains it.

### F10. (LOW, process) Sibling-repo dependencies are local-only; CI is manual

**File:** `STATUS.md`; `.github/workflows/ci.yml`.

LakeCat depends on Sail helper commits (`Expose Iceberg table status
conversion`, `Expose Iceberg planning result helpers`) that are committed locally
but **blocked from pushing** to `lakehq/sail`. The GOAL's repo-boundary
discipline — "push reusable work to Sail, then depend on it" — is only half
realized: the work is pushed *down* into Sail locally but not *upstream*, so the
green dependency graph exists only on this machine, and CI is manual-only. This
is a reproducibility/supply-chain risk, not a code defect, but it should be
tracked explicitly: the build is not independently verifiable until those Sail
commits land upstream (or LakeCat pins a published Sail).

---

## What is genuinely good (and new since OPUS1)

- **The capability type-state is honestly unforgeable** within the crate
  boundary (private fields, single checked constructor) and *every* privileged
  handler consumes it — the governance layer is no longer decorative.
- **The Turso commit is a textbook optimistic-concurrency catalog commit:**
  expected-previous re-check *and* guarded UPDATE, all side-effect rows in the
  same transaction, idempotency replay, and a real concurrent-writer test. This
  is the hardest thing in a catalog and it is right.
- **The outbox cleanly decouples side effects** and gives at-least-once
  projection with a drain API — the architecture's replay/export goal, realized.
- **Boundary discipline held under growth:** graph taxonomy moved into Grust,
  Iceberg conversion/planning helpers moved into Sail, TypeDID attestation into
  TypeSec. LakeCat stayed thin even as it tripled in size. That is the GOAL's
  prime directive, and the commits honor it.

---

## One-line verdict

OPUS1's missing spine — auth, typed capabilities, durable CAS commit,
persistence, real-engine wiring, a CLI — is built and, where it counts, built
correctly; LakeCat is now a governance-*gated* Iceberg catalog. The next and
defining step is to make the gate *shape the data*: derive an enforceable
column/row restriction from ODRL bindings, push it through a Sail-planned read
that flows through the in-process provider, and prove it with the QGLake
"metadata-visible, data-denied" broker. See [OPUS2-DESIGN.md](OPUS2-DESIGN.md).
