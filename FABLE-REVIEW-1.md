# FABLE-REVIEW-1 — LakeCat code & documentation review, improvement plan, execution log

- **Reviewer:** Claude Fable 5 (session 2026-07-03)
- **Baseline:** `master` @ `8d9e8791` (workspace `0.2.1`, post-v0.2.1 "Lynx maintenance")
- **Method:** re-verification of every open finding from the 2026-06-25 OPUS-era
  review against the *current* working tree (three parallel verification passes:
  security/identity, Iceberg-REST spec conformance, docs/release state), plus a
  fresh survey of everything that landed since v0.2.0. All file:line cites below
  are from the current tree, not the pre-refactor monoliths.
- **Relationship to other docs:** this is a point-in-time review + plan, in the
  spirit of the archived `docs/completed/OPUS*.md` files. `DESIGN.md` remains the
  living design surface; items adopted from here should graduate into DESIGN.md
  priorities. Per `docs/completed/README.md` the archive should not grow new
  OPUS-numbered plans — hence the FABLE series.

---

## 1. Where the project stands

Since the 2026-06-25 review baseline, the project has closed most of what that
review called its worst problems:

- **The human-reviewability refactor is complete.** No monolithic sources remain
  (the 58k-line `lakecat-service/lib.rs` era is over; largest prod file today is
  ~2.4k lines); tests live in separate files; 11 crates, ~124k LoC total
  (including the newly extracted `qglake-bundle` wire-format crate, DRY'd
  against qg-rust).
- **Stock-client interop went from "impossible" to a proven, stated goal.** The
  book's Benchmark Suite chapter documents a full stock **PyIceberg 0.11.1**
  round-trip (create → append → scan 1000 rows, no shim) enabled by five fixes:
  object-shaped map fields (old finding **H8 — fixed**), spec-canonical
  `/v1/{prefix}/…` endpoint advertisement, a new `listTables` endpoint, the
  default build now **rejecting** updates it cannot apply instead of silently
  dropping them (old **H9 — fixed**), and Sail's `apply_table_updates` learning
  `add-snapshot`/`set-snapshot-ref`. This resolves old open-question #1
  ("stock-client interop or LakeCat-aware only?") in favor of **stock interop**
  — which raises the bar for the still-open REST-conformance findings below.
- **Dependency posture is dramatically simpler.** Sail is a Cargo git dep on
  `querygraph/sail#lakecat` (locked at `bddb1706`; canonical doc:
  `LAKECAT-SAIL.md`); Grust and TypeSec are **published crates 0.11.0**. No path
  deps, no `ci/sail-patches` bridge.
- **Concurrency and performance work landed:** Turso MVCC concurrent writes
  (`BEGIN CONCURRENT` + bounded retry; the global write mutex is gone — old L3,
  I2, FW-16 fixed), pooled pragma-warmed write connections, per-bucket
  object_store client cache, and the Foyer object-store cache in Sail (~26×
  warm-scan speedup; honest 1.63× engine edge vs Spark).
- **Release discipline is real:** v0.2.0 and v0.2.1 tagged with recorded
  release-candidate proofs; `scripts/check-release-readiness.sh` gates fmt, the
  full feature-matrix test set, book artifacts, dependency contracts, and the
  QGLake handoff proof.

The commit path + Turso CAS + transactional outbox remain the load-bearing,
audited core. Nothing in this review suggests touching their semantics.

## 2. Finding re-verification (what is actually still open)

Every still-open finding was re-verified against the current tree on 2026-07-03.

### Fixed since the prior review (no action needed)

| Old ID | What | How it closed |
| --- | --- | --- |
| H1/M13/M14 | stale release proof/version docs | v0.2.0/v0.2.1 release trains |
| H2, H3, H10 | feature builds red (sail drift, cli fixture, grust skew) | sail git dep + fixes; all-features green |
| H8 | map fields serialized as arrays | `config_map` object adapter in `lakecat-api` |
| H9 | default build silently drops commit updates | `prepare_commit` now returns `NotSupported`. **Residual:** register-style commits on the default build still pass `requirements` through unvalidated (`lakecat-core/src/sail.rs:206`) — see Phase 3.7 |
| M12 | 31k-line CLI monolith | refactor complete |
| L3, I2 | global write mutex; pragmas only on migrate | Turso MVCC + per-connection pragmas |
| M15 | Turso serialization fix undocumented | superseded by MVCC + book chapters |

### Still open — security/identity cluster (all confirmed OPEN)

| ID | Finding | Current evidence |
| --- | --- | --- |
| **H6** | bare `x-lakecat-principal` defaults to `PrincipalKind::Human`, unverified → trivial impersonation | `lakecat-service/src/identity.rs:91` (`unwrap_or(PrincipalKind::Human)`), attestation stamped `"unverified"` (`identity.rs:170`); TypeDID check only runs when an envelope is present (`identity.rs:242`) |
| **H7** | raw-credential exception decided by lakecat's `kind==Human` heuristic, never re-evaluated by TypeSec as its own action | `identity.rs:351` (`trusted_human`), context-only injection at `identity.rs:326`; single `credentials.vend` action (`lakecat-security/src/lib.rs:966`) |
| **M1** | `typesec-local` + no `LAKECAT_TYPESEC_RBAC_POLICY` silently wires allow-all governance **plus** a live secret-ref resolver | `lakecat-service/src/main.rs:99-101`, `:113` — no warning emitted |
| **M2** | allow-all path reports `engine:"typesec"` + computed policy hash, indistinguishable from an enforced allow | `lakecat-security/src/typesec_integration/mod.rs:51-81` |
| **M3** | plan-task HMAC bypassable — unsigned `lakecat:sail-json:` and plain forms still accepted | `lakecat-sail/src/sail_integration/mod.rs:1803-1847` |
| **L11** | hardcoded default plan-task signing key, silent fallback | `sail_integration/mod.rs:35`, `:1894-1898` |

### Still open — Iceberg-REST / correctness cluster (all confirmed OPEN)

| ID | Finding | Current evidence |
| --- | --- | --- |
| **H4** | createTable returns a `metadata_location` never written to storage (404s until first commit) | `lakecat-service/src/handlers.rs:382`,`:401`; only writer is `location.rs:70`, called solely from `commit.rs:75` |
| **H5** | book onboarding uses port **3000**; service binds **8181**; `LAKECAT_BIND_ADDR` undocumented | 15× `:3000` in `docs/book/lakecat.md` (`:921`,`:1069`,…); `main.rs:49` |
| **M4** | createTable auto-creates missing namespaces instead of 404 `NoSuchNamespace` | memory `memory.rs:186-190`; turso `insert or ignore` `turso_store/mod.rs:361-369` |
| **M5** | duplicate createNamespace hidden (200, not 409 `AlreadyExists`) — breaks pyiceberg's `create_namespace_if_not_exists` | `memory.rs:71-76`; `turso_store/mod.rs:200-208` |
| **M6** | error `type` always `"LakeCatError"`; enum too coarse; authz denial returns **409** not 403 | `lakecat-service/src/error.rs:32`; `lakecat-core/src/lib.rs:14-24`; `identity.rs:342` |
| **M7** | v4-extension requirement validation counts non-`main` ref assertions as validated without checking; unknown requirement types fall through `_ => {}` | `sail_integration/mod.rs:582-595`, `:634` |
| **L7** | default createTable location hardcodes `file:///tmp/lakecat/…`, ignoring storage profile | `handlers.rs:362-369` |
| **L8** | `.` allowed in name components → namespace aliasing | `lakecat-core/src/lib.rs:247` |
| **L10** | fragile `unwrap()`s in CLI view-receipt admission | `lakecat-cli/src/verify_receipts.rs:741-751` |
| **L12** | CLI URL path segments raw-interpolated, no percent-encoding | `lakecat-cli/src/main.rs:103,121,140,159`; `http.rs:6-7` |
| **FW-17** | no background outbox drain; projection advances only via `POST /management/v1/lineage/drain` | `outbox.rs:38`,`:107`; `router.rs:166` |

Also carried forward, unchanged status: M8 (context-blind TypeSec `check`), M9
(local ODRL parsing vs `typesec-odrl` — partially reframed by DESIGN F2's
fail-closed subset work), M10 (hand-rolled file pruning), M11 (decimal/timestamptz
lossiness), L1/L2/L4/L6/L9/L13/L14/L15, I1.

### New observations from this review (N-series)

| ID | Observation |
| --- | --- |
| **N1** | **Release proof is stale vs HEAD**: recorded proof head `b6ade047` (STATUS.md:20, README.md:189) predates executable changes (crate sources, `Cargo.lock` Sail bump, scripts) — `check-release-readiness.sh` freshness rule would flag it. Fine mid-cycle; must be re-run at next release. |
| **N2** | `RELEASE.md` is still framed around the v0.1.0 first-release checklist; hasn't been reframed for the 0.2.x train (old L5 residue). |
| **N3** | `GOAL.md` "Current Stage" pins Grust `0.10.0` / TypeSec `0.8.0`; the workspace actually consumes both at published `0.11.0`. |
| **N4** | `CLAUDE.md` is dominated by a superseded session checkpoint (~2/3 of the file is explicitly historical); worth compacting so agent context spends tokens on live facts. |
| **N5** | The default-features `cargo test --workspace` gate passes at the review baseline; the feature matrix did **not** (see N7). |
| **N6** | `qglake-bundle` (new crate) is thin and single-purpose; no findings, but it inherits the L13 canonical-JSON hazard wherever it hashes. |
| **N8** | **`scripts/check-local-dependency-contract.sh` was red at HEAD** (found during this review's release-gate run): commit `5c3cbc9e` flipped the handoff verifier to tolerate additive importer verification fields (renaming `…_rejects_extra_verification_fields` to `…_tolerates_extra_root_fields`) but did not update the contract guard that greps for the old test name. Fixed here — the guard now requires the tolerant root test plus the still-strict table/view record tests. Same lesson as N7: the contract scripts are part of the per-change gate. |
| **N7** | **`cargo test -p lakecat-sail --all-features` was red at HEAD** (found and fixed during this review's Phase 1): the H9 fix commit `183033f9` made the deferred engine reject update-carrying commits, breaking `provider_resolves_governed_tables_in_process`, which drives exactly such a commit through `DeferredSailCatalogEngine`. The commit was verified on default features only; nothing between it and this review ran the feature-gated suites. This is the FW-3 pattern recurring — per-change gates must include the feature matrix (or at least the crates whose feature-gated tests consume the changed seam), not only the release gate. |

## 3. The plan

Structured as four phases: each item names the findings it closes, the concrete
change, and the gate that proves it. Phase 1 is implemented in this pass (see
§5 execution log). Phases 2–4 are ordered future work.

### Phase 1 — implemented now (spec conformance + honesty, no maintainer decision needed)

**1.1 Docs truth (closes H5, N3; FW-6).**
Replace all 15 `127.0.0.1:3000` occurrences in `docs/book/lakecat.md` with
`127.0.0.1:8181` and document `LAKECAT_BIND_ADDR` at first use; update GOAL.md's
dependency sentence to Grust/TypeSec 0.11.0. Book *source* only — tracked book
artifacts are rebuilt at release per GOAL.md. Gate: grep, `git diff --check`.

**1.2 Iceberg-conformant error model (closes M6; FW-8 core).**
Extend `LakeCatError` with `AlreadyExists { object, name }` (→ 409
`AlreadyExistsException`) and `Forbidden(String)` (→ 403 `ForbiddenException`);
map the existing variants to spec exception types in the REST envelope:
`InvalidArgument`→400 `BadRequestException`; `NotFound{object}`→404
`NoSuchTableException`/`NoSuchNamespaceException`/`NoSuchViewException`
(entity-aware, generic `NotFoundException` otherwise); `Conflict`→409
`CommitFailedException`; `NotSupported`→501 `UnsupportedOperationException`;
`Internal`→500 `InternalServerError`. Authorization denial switches from
`Conflict` (409) to `Forbidden` (403). Update the exhaustive matches
(`error.rs`, `identity.rs` redaction, `location.rs` cleanup-context,
`tests/common.rs`) and the 409-assertions that were really authz denials.
Gate: `cargo test -p lakecat-service`.

**1.3 Namespace existence/duplicate semantics (closes M4, M5; FW-11).**
Store-level, both backends, atomic: `create_namespace` returns
`AlreadyExists` on duplicates (memory: occupancy check; turso: plain `insert`
mapping the unique-violation); `create_table` requires the namespace to exist
(→ `NotFound{object:"namespace"}` → 404 `NoSuchNamespaceException` on the
wire). Update store/service/CLI-fixture tests to create namespaces explicitly.
Gate: `cargo test -p lakecat-store` (default + `turso-local`),
`-p lakecat-service`, CLI fixture suite.

**1.4 Commit-requirement + signing guardrails (closes M7; first slice of M1, L11).**
In `validate_v4_extension_commit_requirements`: **fail closed** on non-`main`
`assert-ref-snapshot-id` (`NotSupported` — the v4 JSON summary carries no
`refs` map, and extending it would perturb hash-sensitive commit-plan
evidence; the typed ≤v3 path already validates refs via `metadata.refs`), and
reject unknown requirement types (`_ => {}` becomes an `InvalidArgument`).
Emit loud startup warnings when (a) `typesec-local` wires allow-all governance
with a live secret-ref resolver and (b) the plan-task signing key falls back
to the hardcoded default. Warnings are the non-breaking slice; the fail-closed
config behavior is Phase 2 (needs the maintainer decisions below). Gate:
`cargo test -p lakecat-sail --all-features`.

### Phase 2 — identity & credential hardening (needs maintainer decisions; highest security value)

These four hinge on open questions the maintainer must settle (§4). Proposed
defaults in parentheses.

- **2.1 (H6; FW-2)** Stop defaulting bare `x-lakecat-principal` to trusted
  Human. (Proposal: require `LAKECAT_TRUST_PRINCIPAL_HEADER=1` — the documented
  "behind an authenticating proxy" posture — else bare principals get
  `PrincipalKind::Agent`-level trust and no raw-credential exception; TypeDID
  envelopes remain the verified path.)
- **2.2 (H7; FW-10)** Make raw-vs-governed vending a distinct TypeSec action
  (`credentials.vend-raw`) so the engine — not a lakecat heuristic — decides
  the exception; keep the audited context.
- **2.3 (M1, M2; FW-9)** `typesec-local` with no policy fails closed unless an
  explicit demo flag (`LAKECAT_TYPESEC_ALLOW_ALL=demo`) is set; the allow-all
  receipt gets a distinct engine label (e.g. `typesec-allow-all`). Coordinate
  the receipt-label change with qg-rust before landing (evidence shape).
- **2.4 (M3, L11 full; FW-12)** Reject unsigned plan-task token forms unless a
  compatibility flag is set; refuse to start `sail-local` with the default
  signing key outside a dev flag.

### Phase 3 — catalog completeness & robustness

- **3.1 (H4, L7; FW-7)** Persist the synthesized initial `metadata.json` on
  createTable via the existing `write_planned_metadata` (cleanup on failure),
  and derive the default location from the warehouse storage profile instead of
  `file:///tmp`. Depends on maintainer question #3 (eager-write contract).
- **3.2 (FW-17)** Background outbox drain task in `main.rs` (interval + backoff
  + env toggle, off in tests), or explicitly document the operator-polling
  contract. The drain loop must reuse `drain_outbox_once` untouched.
- **3.3 (L12; FW-21)** Percent-encode CLI URL path segments (single helper in
  `http.rs`, applied at every path construction; Iceberg multipart-namespace
  `%1F` encoding where applicable).
- **3.4 (L8; FW-19)** Disallow `.` in namespace components/table names (or
  escape per Iceberg's 0x1F convention); reject empty components.
- **3.5 (L10, L4, L6/I1; FW-23/24/25)** CLI receipt-admission `unwrap` →
  fail-closed match; move `table.created` audit/outbox into the create
  transaction; align soft-delete replay + duplicate-audit-id behavior across
  backends.
- **3.6 (FW-16 residue)** Add a live-HTTP CLI test and a cross-backend
  (memory/turso) contract test run; the multi-thread commit tests exist, the
  cross-process/live-wire coverage still doesn't.
- **3.7 (H9 residual)** Validate commit `requirements` against the current
  metadata on the default build's register-style path too — the deferred
  engine currently forwards them unchecked (`lakecat-core/src/sail.rs:206`);
  the JSON-summary validator in `sail_integration` shows the shape, but the
  reusable home for it is a small metadata-summary check that doesn't need
  `sail-local`.

### Phase 4 — platform, upstreaming, release ops

- **4.1 (M10, M11; FW-13/14)** Push file/manifest pruning and typed schema
  conversion (decimal P/S, timestamptz) into Sail on the `lakecat` branch;
  shrink the branch by rebasing onto upstream as equivalents land
  (`LAKECAT-SAIL.md` is the runbook).
- **4.2 (L13; FW-18)** Adopt RFC-8785/JCS canonical hashing **or** pin a
  golden-hash fixture shared with qg-rust (now also covering `qglake-bundle`,
  N6); add a guard that serde_json `preserve_order` stays off.
- **4.3 (M8, M9; FW-20)** Pass request context into TypeSec
  (`check_with_context`) when a context-aware engine is wired; either adopt
  `typesec-odrl` for the local ODRL subset or mark the local parser as the
  intentional enforceable-subset bridge in DESIGN.md (F2 already trends there).
- **4.4 (N1, N2, N4; FW-5 pattern)** Release ops: re-run the full
  release-candidate gate to refresh the proof at the next release point;
  reframe `RELEASE.md` around the current train; compact `CLAUDE.md` to live
  facts. FW-30 roadmap (0.3 → 1.0 Lion) remains the cadence.
- **4.5 (FW-27/28)** Keep v4 JSON passthrough an explicit bridge until typed
  Sail v4; cloud-SDK secret resolvers; move remaining side effects to the
  outbox.

## 4. Open maintainer decisions (carried forward, renumbered)

1. **Proxy posture (drives 2.1/2.2):** is bare `x-lakecat-principal` → trusted
   Human acceptable behind an authenticating proxy, or must LakeCat verify?
2. **Demo posture (drives 2.3):** may `typesec-local` without a policy ever run
   allow-all with a real secret resolver, and under what flag?
3. **createTable contract (drives 3.1):** persist initial metadata eagerly, or
   document "location resolves after first commit"?
4. **Namespace grammar (drives 3.4):** should `.` be allowed in components at all?
5. **Hashing contract (drives 4.2):** does qg-rust expect byte-matched
   serde_json or JCS? Golden fixture?

## 5. Sibling checkpoint review (2026-07-03, post-merge)

The parallel Fable work streams in the sibling repos checkpointed after this
review's Phase 1 merged. State and LakeCat implications:

- **grust** — branch `full39075`, clean, workspace `0.11.0` (unpublished
  branch work): GQL read-reference features F8–F11 (CALL subqueries,
  table-valued functions, shortestPath/allShortestPaths, backend-native
  passthrough), atomic batch transactions, an executable portable read corpus,
  and a plan for GQL_PUSHDOWN2 (lowering F8–F10 into SQL pushdown). All
  additive; nothing LakeCat consumes (published `grust-graph`/`grust-turso`
  0.11.0) changes. Future value: richer graph queries over the catalog
  projection (P4) once a version ships with the GQL surface.
- **typesec** — `main`, clean, **24 commits past the `v0.11.0` tag**
  (unreleased): signed decision receipts + decision logging/replay
  (in `typesec-integrations`), JSON-Schema tool-argument validation, OTel
  audit sink, policy-aware tool listing, `#[typesec_tool]`, typesec-wasm, an
  enforcement proxy, capability lease attenuation, conversation typestate, and
  a PyPI package. Two LakeCat hooks: (1) **Phase 2.3's honest receipts should
  adopt `typesec-integrations` signed decision receipts** when published,
  rather than growing a local labeling scheme (the FW-9/FW-10
  boundary-correct home); (2) M8 is *not* unblocked — the published
  `typesec-rbac` engine LakeCat calls still exposes context-free
  `check(subject, action, resource)` (`typesec-rbac/src/engine.rs:107`); the
  RequestContext work targeted the Python agent stack. Phase 4.3 still needs
  an upstream engine seam.
- **querygraph/qg-rust** — released **0.3.0 "Goshawk"** (the previously
  uncommitted WIP is now committed), plus a `/v1/answer` server slice and
  **TypeDID envelope auth on governed `/v1` routes**. Its suite passes
  **38/38 against the merged LakeCat path deps** (`lakecat-core`,
  `qglake-bundle` 0.2.1). The TypeDID-auth direction is an ecosystem signal
  that strengthens Phase 2.1/2.2: when LakeCat stops trusting bare principal
  headers, qg agents already carry verifiable envelopes.

**Dependency verdict (unchanged):** LakeCat stays on published Grust/TypeSec
`0.11.0` and the Sail `lakecat`-branch pin. Both siblings' checkpoints are
unreleased; bump when they publish, and revisit Phase 2.3 at the next TypeSec
release.

## 6. Execution log (this session)

Implemented on branch `fable/review-1`, one commit per unit, CHANGELOG.md
updated per AGENTS.md convention. Filled in as units land:

- [x] **1.1 Docs truth** — book examples now use `127.0.0.1:8181` and document
  `LAKECAT_BIND_ADDR`; GOAL.md dependency posture updated to Grust/TypeSec
  0.11.0 (tracked book artifacts intentionally not rebuilt — release action).
- [x] **1.2 Iceberg error model** — `AlreadyExists`/`Forbidden` variants added;
  entity-aware Iceberg exception types on the wire; authz denial now 403
  `ForbiddenException`.
- [x] **1.3 Namespace semantics** — duplicate createNamespace → 409
  `AlreadyExistsException`; createTable in a missing namespace → 404
  `NoSuchNamespaceException`; both backends, atomic at the store.
- [x] **1.4 Guardrails** — non-`main` ref assertions on the v4-extension path
  fail closed (`NotSupported`); unknown requirement types rejected
  (`InvalidArgument`); startup warnings for allow-all governance wiring and
  the default plan-task signing key.
- [x] **N7 repair (unplanned)** — fixed the pre-existing all-features redness:
  the H9-fallout provider commit test and three provider scan tests missing
  namespace setup.

Gates run green: `cargo fmt --check` (touched crates); `cargo test -p
lakecat-store` default **67** + `turso-local` **189** (65/185 baseline + 4 new
behavior tests); `-p lakecat-service` default **453** + `turso-local` (450
post-listTables baseline + 3 new wire-shape tests); `-p lakecat-sail
--all-features` **31** (29 baseline + 2 new fail-closed tests); `-p lakecat-cli`
default **492** + the `qglake-fixture` focused suite; `cargo test --workspace`
(default); `git diff --check`. ~58 existing tests updated for the namespace
semantics; #[test] counts preserved everywhere except the 9 deliberately new
tests.
