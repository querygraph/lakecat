# LakeCat — agent context

Rust-native, Iceberg-compatible **REST catalog foundation** for QueryGraph
(workspace `0.1.1`, edition 2024). The catalog boundary is deliberately **thin**:
identity/tenancy, Iceberg REST compatibility, metadata-pointer state, policy
gates, and integration events live here; reusable semantics are pushed to
siblings — **Sail** (`../sail`: Iceberg format/scan/pruning/engine), **Grust**
(`../grust`: graph), **TypeSec** (`../typesec`, published `0.8.0`:
governance/policy/receipts), **QueryGraph** (`../querygraph`: end-to-end target).

**Binding guidance:** `AGENTS.md`, `GOAL.md`, `DESIGN.md` (living design surface).
This file records the state of a **full project + book review (2026-06-25)** and
the **human-reviewability refactor** that follows from it. Don't duplicate
AGENTS.md/GOAL.md — read them first.

> **Live state:** see **SESSION CHECKPOINT** immediately below. The
> "Build & verification status (as of 2026-06-25 review)" section further down is
> the *review-time* snapshot; the checkpoint supersedes it for current state.

---

## 🔗 Sail dependency: the `lakecat` integration branch (READ THIS)

LakeCat's `../sail` path-deps build from **whatever `../sail` is checked out on**.
LakeCat's `sail-local` / `catalog-provider` features need Sail changes that are NOT
in upstream `lakehq/sail` main, so:

- **`../sail` MUST be on branch `lakecat`** for LakeCat to build those features.
  Switching `../sail` to `main`/another branch breaks `sail-local`/`catalog-provider`
  (and silently regresses behavior). Use a `git worktree` for other Sail work.
- **`sail:lakecat`** = upstream `lakehq/sail` main + the minimal LakeCat-needed
  commits, in order: (1) `apply_table_updates` (Iceberg metadata evolution; needs
  the merged #2134 `TableUpdate` enums), (2) manifest `lower/upper_bounds` Avro
  round-trip fix, (3) pruning type-mismatch guard, (4) the Iceberg planning/`models`
  exposure + `CatalogProvider` commit-table seam. Pushed to `fork/lakecat` (alexy's
  fork) for durability — **not a PR**.
- **Why a branch, not PRs:** the Sail maintainers are actively redesigning catalog/
  table internals and asked that uncoordinated PRs wait (PRs #2139 bounds, #2140
  catalog seam are now **draft** upstream as coordination/bug-report artifacts, NOT
  LakeCat's build dependency). The `lakecat` branch decouples LakeCat's velocity from
  the upstream PR timeline.
- **Maintenance:** rebase `sail:lakecat` onto upstream `main` periodically; the
  branch shrinks as the maintainers land equivalents or LakeCat aligns to Sail's
  catalog API. When everything LakeCat needs is in upstream `main`, retire the
  branch and the path-deps build against `main`.
- **Toolchain:** never run `cargo +nightly` (per AGENTS.md). Sail CI uses nightly
  fmt; let Sail's CI handle that, don't run it locally.

---

## 🟢 SESSION CHECKPOINT — resume from here (2026-06-25, handoff to CLI)

A prior session did the review + started the refactor, then **stopped for handoff
to a CLI session**. Read this section first; the per-session task list does not
persist, so this IS the task list.

### Working-tree state (READ THIS)
- **Branch: `master`. All work is UNCOMMITTED.** Nothing has been committed. The
  refactor lives as modified tracked files (`lib.rs`/`main.rs` shrunk) + many
  **untracked** new module files. They persist on disk; a CLI session resumes from
  this same working tree. **Recommendation: create a branch and commit per logical
  unit (AGENTS.md convention) with CHANGELOG.md entries before doing more.** Do NOT
  commit on `master`. Committing moves the release-proof head (see release-proof
  caveat) — confirm intent first.
- `cargo fmt` is **clean** across touched crates. `git diff --check` not yet run.

### What is DONE (green)
1. **Step 0 build fixes (partial):**
   - ✅ **H3** fixed: `lakecat-cli` `qglake-fixture` `CreateTableRequest` literal
     (`ensure_qglake_table`) — compiles under `--features qglake-fixture`.
   - ✅ `lakecat-sail` dead import removed + `cargo fmt` → **default build green, 0
     warnings**.
   - ⛔ **H2 BLOCKED, needs a `../sail` decision** (see below). `sail-local` /
     `catalog-provider` still don't compile (pre-existing; not caused by refactor).
2. **`lakecat-cli` refactor — COMPLETE & verified green.** `main.rs` 31,354 → 201
   lines. Modules: `cli, commands, http, lineage, replay_evidence, verify_handoff,
   verify_proof, verify_receipts, verify_replay, fixture` + `tests/` tree (16
   files: `mod, common`, topic files). 500 `#[test]` preserved; **492 pass default
   / 499 pass `qglake-fixture`**. Longest fn 537 → 103 lines. DRY: `lakecat_endpoint`,
   `send_request` in `http.rs`.
3. **`lakecat-service` refactor — STRUCTURE COMPLETE, default green; finish-up
   pending.** `lib.rs` 58,105 → **41 lines** (thin root, glob re-exports). Modules:
   `state, identity, commit, outbox, scan, location, router, responses, error,
   handlers, lineage_summary, typesec_credential_issuer, typesec_typedid,
   evidence/{mod,consts,fields,core_receipts,table_scan,management,credentials_view,
   outbox_evidence}` + `tests/` tree (18 files). Largest prod file `handlers.rs`
   1,697 lines (no monsters). **`cargo check -p lakecat-service` exit 0; `cargo test
   -p lakecat-service` = 448 passed / 0 failed (== baseline).**
   - ✅ **FINISH-UP COMPLETE (this session):** (a) **23 unused-import warnings cleaned**
     (`cargo fix` was too conservative — manually removed; all confirmed unused across
     *every* feature via raw-text grep incl. inside `#[cfg]` blocks, none were
     `pub use`). `cargo check -p lakecat-service` = **0 warnings**; `cargo fmt` clean.
     (b) **Feature-gated parity re-verified:** default **448**, `turso-local` **448**,
     `typesec-local` **467** (+19), all 0 warnings. Test fns: original HEAD `lib.rs`
     472 → refactored tree 477; **name-diff proves 0 tests lost** (+5 are
     pre-existing feature-gated typesec/grust-turso governance tests already in the
     modified working tree). **⛔ `grust-turso-local` is RED — but PRE-EXISTING sibling
     drift, NOT a refactor regression** (new finding **H10**, below). The dependency
     `grust-cypher` fails to compile *before* lakecat-service's own code is reached;
     it builds fine standalone in `../grust` but a feature/version skew (`cypher` +
     `memory` features pulled via the path `grust-graph`) breaks it as consumed from
     lakecat. The 2026-06-25 review's build matrix never covered this gate.

### NEW build finding (this session)
- **H10 · build/feature-gate · ✅ RESOLVED (self-resolved; re-verified)** — was:
  `grust-turso-local` failed because the path `grust-cypher 0.10.0` got compiled
  against the crates.io `grust-core 0.9.1` (pulled transitively by published
  `typesec-rbac 0.8.0`), a stale dependency-resolution artifact. After the committed
  `Cargo.lock` re-resolution + clean rebuilds it no longer reproduces:
  `cargo test -p lakecat-service --features grust-turso-local` = **451 passed**
  (448 + 3 grust-turso), `--no-run` compiles clean. grust/grust-graph/grust-turso
  also build standalone (incl. `cypher,memory`). No `../grust` edit was needed.

### Sibling-commit investigation (this session) — H2 is NOT pinnable
Exhaustive `../sail` history search: the symbols `lakecat-sail` needs were spread
across **never-merged fork branches** and the current main line **redesigned the
`CatalogProvider` trait** (now `crates/sail-catalog/src/provider/mod.rs`). No single
sail commit has them all — so H2 cannot be fixed by pinning `../sail` to a commit.
Precise gaps vs sail HEAD `d66a2676`:
- **`sail-local` (only 2 errors — small):** (a) `sail_catalog_iceberg::models` is now
  private (`mod models;`); lakecat imports `sail_catalog_iceberg::models::…`. (b) the
  two planning helpers `completed_planning_with_id_result_from_values` /
  `fetch_scan_tasks_result_from_values` are absent. **Both are exactly the existing
  user-authored commit `fdb3b657 "Expose Iceberg planning result helpers"`** (adds
  `sail-catalog-iceberg/src/planning.rs` + `pub use`; `planning.rs` is a clean add on
  HEAD). Fix = cherry-pick `fdb3b657` onto sail HEAD + `pub mod models` (or targeted
  `pub use crate::models::{…}`).
- **`catalog-provider` (4 errors — larger):** needs `fdb3b657` (planning) PLUS
  `load_table_result_to_status` as a crate-root `pub use` (it exists as a provider
  method at `sail-catalog-iceberg/src/provider.rs:540`, parent fork commit
  `68631016`) PLUS a `CatalogProvider`-trait seam that **never cleanly existed**:
  `commit_table`/`get_table_commits` methods + `CommitTableOptions`/
  `GetTableCommitsOptions`/`GetTableCommitsResponse`/`TableCommitInfo` types. Current
  sail has `commit_table_request` (`provider.rs:301`) instead. So catalog-provider
  needs either authoring that seam in sail or porting lakecat-sail's `catalog_provider`
  module to the new API.
**Conclusion:** no commit to pin. Resolve by (1) **surgical sail edits** — cherry-pick
`fdb3b657` (+ expose `models`/`load_table_result_to_status`, author the commit-table
seam) — or (2) **port lakecat-sail** to current sail. `../grust` (H10) needs nothing.

### ✅ H2 RESOLVED via minimal sail exposure (this session; user chose this)
A pure lakecat-only port was infeasible: sail's `models` module is fully private with
no public path, and the planning helpers return `models::*` types — a lakecat port
would mean duplicating dozens of generated REST model structs (anti-AGENTS.md). So
the boundary-correct minimal sail exposure was done. **`../sail` working-tree changes
(uncommitted — need committing in the sail repo; on branch `claude/table-update-apply`):**
- `sail-catalog-iceberg/src/lib.rs`: `mod models` → **`pub mod models`**; add
  `mod planning;` + `pub use crate::models::{LoadTableResult, TableMetadata}` +
  `pub use planning::{…}` + `load_table_result_to_status` to the provider re-export.
- `sail-catalog-iceberg/src/planning.rs`: **new** (verbatim from user's `fdb3b657`).
- `sail-catalog-iceberg/src/provider.rs`: extracted the `load_table_result_to_status`
  *method* into a standalone `pub fn (catalog, table_name, database, result)`; the
  method now delegates. (Only `self` use was `self.name` → the `catalog` param.)
- `sail-catalog/src/provider/{options.rs,mod.rs}`: added `CommitTableOptions`,
  `GetTableCommitsOptions` (incl. `table_uri`), `TableCommitInfo`,
  `GetTableCommitsResponse`; added `CatalogProvider::{commit_table, get_table_commits}`
  as **default methods returning `NotSupported`** (so all existing sail providers
  still compile — verified).
**lakecat side (committed on this branch):** updated 3 test `CreateTableOptions`
constructions from removed `if_not_exists`/`replace` to `mode: CreateTableMode::Create`.
**Status:** `cargo build -p lakecat-sail --features sail-local` ✅, `--features
catalog-provider` ✅, `sail-catalog`/`sail-catalog-iceberg` build ✅. `cargo test -p
lakecat-sail --all-features` = **28 passed / 1 failed**.
- ✅ **The 1 failure was ROOT-CAUSED + FIXED upstream in sail (real bug):**
  `IntBytesMapEntry.value: Vec<u8>` (manifest `_serde.rs`) is typed Avro `bytes`, but
  serde's default `Vec<u8>` serialization emits an Avro **array**, which fails to
  resolve against the `bytes` schema → the nullable `lower_bounds`/`upper_bounds`
  maps were silently written as **null**, dropping every column bound on round-trip.
  Fixed with byte-string (de)serialization on the entry value (tolerating the legacy
  array form on read). This is a genuine `sail-iceberg` bug affecting ALL Iceberg
  manifest bounds, not just LakeCat. **`lakecat-sail --all-features` = 29 passed / 0
  failed; `sail-iceberg` own tests = 79 passed.**

### ✅ H2 FULLY RESOLVED — `../sail` committed (2 commits on `claude/table-update-apply`)
- `32dbf172 fix(iceberg): round-trip manifest lower/upper bounds through Avro`
- `b09d7bda feat(catalog): expose Iceberg planning helpers + commit-table provider seam`
Both `sail-local` and `catalog-provider` now build AND test green.

### ✅✅ `cargo test --workspace --all-features` PASSES (EXIT 0) — the headline H2/H3 gate
The full all-features workspace test — red all session (H2/H3) — is now **green, 0
failures**. Fixes that got it there: H3 (cli, earlier), H2 (sail exposure + the
manifest-bounds bug fix), and one refactor-induced visibility nit
(`CasRaceStore::new` → `pub(crate)`, only compiled under all-features).
- ⚠️ **`../sail` branch fragility:** lakecat builds from `../sail`'s *working tree*.
  Mid-session it got switched to `claude/table-update` (which lacks both my changes
  AND `apply_table_updates`), breaking the build; I restored it to
  **`claude/table-update-apply`** (HEAD now my `b09d7bda`). If sail is switched off
  that branch, lakecat-sail's feature builds break again.
- Minor: ~18 unused-import **warnings** under all-features in `lakecat-service`
  (feature-combination leftovers) — don't fail the gate; clean later.

### Remaining proof blocker (only one left)
**Book taxonomy/ledger/readiness drift** — 3 `check-local-dependency-contract.sh`
guards expect sections in `docs/book/lakecat.md` that the `bbdaa0bb` book rebuild
removed. Pre-existing, unrelated to H2/H10. Needs a deliberate book reconciliation
(GOAL.md: book rebuild is a release action), then the full `--release-candidate`
gate can run + the proof ref can advance.

### What is NEXT (in order)
4. ✅ **DONE — `lakecat-service` finished** (warnings cleaned + feature-gated parity
   verified; grust-turso-local blocked by pre-existing sibling H10, above).
5. **Refactor `lakecat-store`** (~16.9k LoC). **�️ STRUCTURAL SPLIT DONE (this
   session), verified green:** `lib.rs` **16,852 → 3,802 lines**. Extracted (verbatim,
   path-preserving — `turso_store` already used explicit `crate::` paths;
   `memory_tests` used `use super::*`, both kept as direct root children):
   - `src/turso_store/mod.rs` (2,278 — turso backend prod) + `src/turso_store/tests.rs`
     (6,762 — was the inner `#[cfg(test)] mod tests`).
   - `src/memory_tests.rs` (4,007 — was `#[cfg(test)] mod memory_tests`).
   - `lib.rs` now holds crate imports + `CatalogStore` trait + all record types +
     `MemoryCatalogStore` + free helpers + the two `mod` decls.
   **Gates:** `cargo check -p lakecat-store` (default) **0 warnings**; `--features
   turso-local` **0 warnings**; `cargo test`: default **65**, `turso-local` **183**
   (== baseline). `cargo fmt` clean. **Test-fn count 183 → 183 (0 lost).**
   - ✅ **`write_txn` DRY helper landed as part of step 6 (MVCC, below).** All 15
     write methods now route through it; the `write_guard`/`write_lock` are gone.
   - ⏳ **REMAINING in step 5 (optional):** further split of the ~3.8k root `lib.rs`
     into `records.rs` / `memory.rs` / `helpers.rs` (lower-value; needs `pub(crate)`
     bumps on moved free fns + glob re-export — `turso_store` imports ~15 of them via
     `crate::{…}`).

### ✅ Step 6 — Turso MVCC concurrent writes: DONE & PROVEN (this session)
Implemented per the evidence-based design (BEGIN CONCURRENT, since the spike proved
plain `conn.transaction()` stays single-writer under mvcc). In `turso_store/mod.rs`:
- **`write_txn<T, F>`**: `connect()` → `apply_write_pragmas` (`journal_mode=mvcc` +
  `busy_timeout`, best-effort/ignored as before) → `execute_batch("BEGIN CONCURRENT")`
  → run body on `&Connection` → `execute_batch("COMMIT")`; on a retryable conflict at
  commit (`Write-write conflict`/`Busy`/`BusySnapshot`, classified by
  `is_retryable_conflict(&turso::Error)`) → `ROLLBACK` + exponential `backoff` +
  retry, capped at `WRITE_TXN_MAX_ATTEMPTS=8`. Body signature is
  `for<'c> FnMut(&'c Connection) -> Pin<Box<dyn Future<Output=LakeCatResult<T>> + Send + 'c>>`
  (`WriteTxnFuture` alias) — boxed + `Send` to satisfy `#[async_trait]` and be
  re-runnable. **Async closures (`AsyncFnMut`) did NOT work** — they hit a
  `Send is not general enough` HRTB error under `#[async_trait]`; the boxed-future
  form is required. Re-runnability pattern: closure is `move`, clones owned inputs
  per attempt (`let x = x.clone();`) and the inner `async move` copies references —
  never moves data the retry needs.
- **`write_guard()` + the `write_lock: Arc<Mutex>` field REMOVED.** `migrate()` now
  uses `journal_mode=mvcc` via `apply_write_pragmas`.
- The 3 `&Transaction` helpers (`latest_turso_view_receipt_evidence`,
  `latest_turso_view_receipt_hash`, `tx_insert_outbox_event`→renamed
  `insert_outbox_event`) now take `&Connection`.
- **Same-table race correctness:** loser gets `Write-write conflict` at COMMIT →
  `write_txn` retries → on the new snapshot the metadata-pointer CAS pre-check
  mismatches → terminal `Conflict` (409). So bounded retry converges to exactly one
  winner + Conflicts (no livelock).
- **FW-16 proof (2 new file-backed, multi-thread tests in `turso_store/tests.rs`):**
  `turso_concurrent_commits_to_distinct_tables_all_succeed` (8 tables, all Ok, no
  "database is locked") and `turso_concurrent_commits_to_same_table_yield_one_winner`
  (1 Ok + 7 `Conflict`).
  **Gates:** `cargo test -p lakecat-store`: default **65**, `turso-local` **185**
  (183 baseline + 2 FW-16). 0 warnings, fmt clean. (Also fixes finding **I2** —
  pragmas now apply to every write connection, not just `migrate()`.)

### Turso MVCC — corrected facts (verified against pinned `turso 0.7.0-pre.10` this session)
- The binding `turso::Error` (`turso-0.7.0-pre.10/src/lib.rs:85`) has **`Busy(String)`
  and `BusySnapshot(String)` variants but NO typed `WriteWriteConflict`**. The
  `From<TursoError>` catch-all routes it to **`Error::Error(String)`**. `turso_core`
  `LimboError::WriteWriteConflict` Displays as **`"Write-write conflict"`**
  (`turso_core-0.7.0-pre.10/error.rs:84`). So the retry classifier must be:
  `matches!(err, Busy(_) | BusySnapshot(_)) || matches!(err, Error::Error(m) if
  m.contains("Write-write conflict"))` — **not** a typed-variant match. (Also consider
  `"Commit dependency aborted"`, `error.rs:86`.)
- `turso_error()` (`turso_store/mod.rs:2240`) flattens ALL errors to
  `LakeCatError::Internal` — so retry MUST be decided at the raw `turso::Error` layer
  (inside `write_txn`), before mapping. `is_unique_violation` (`:2236`) matches
  `Error::Constraint` → stays the terminal `Conflict` path (do NOT retry it).
- Pragmas (`migrate()`, `turso_store/mod.rs:~80`) currently set `journal_mode=WAL` +
  `busy_timeout=10000` only on the migrate conn (finding I2). MVCC: switch to
  `journal_mode=mvcc` and apply both on **every** `connect()`.

### Turso MVCC — EMPIRICAL SPIKE RESULTS (this session; resolves the open question)
Ran a 2-writer file-backed probe (A holds tx ~300ms, B acts ~50ms in) across the
matrix. Decisive outcomes:
| journal | begin style | different rows | same row |
|---|---|---|---|
| `wal` | `conn.transaction()` | B = `database is locked` | B = `database is locked` |
| `mvcc` | `conn.transaction()` | B = `database is locked` | B = `database is locked` |
| `mvcc` | **raw `BEGIN CONCURRENT`** | **A=Ok, B=Ok** | A=`commit: Write-write conflict`, B=Ok |

**ANSWER to the open question:** the binding's typed `conn.transaction()` issues
`BEGIN DEFERRED` and **stays single-writer even under `journal_mode=mvcc`** — a
second writer to a DIFFERENT row still fails `database is locked`. MVCC concurrency
requires issuing **`BEGIN CONCURRENT` explicitly via `conn.execute_batch`** (the
typed `TransactionBehavior` enum has only Deferred/Immediate/Exclusive — no
Concurrent). With `mvcc` + `BEGIN CONCURRENT`: different-row commits run truly
concurrently; a same-row race yields exactly one winner and the loser gets
**`Write-write conflict` at COMMIT** (not eagerly at insert, in this pre-release).

**Revised implementation (evidence-based, supersedes the checkpoint's MVCC spec):**
1. `migrate()` + every write connection: `PRAGMA journal_mode=mvcc; PRAGMA
   busy_timeout=…;`.
2. New `write_txn` does NOT use `conn.transaction()`. It: `connect()` → set pragmas
   → `execute_batch("BEGIN CONCURRENT")` → run the body on `&Connection` (the
   existing bodies use `tx.execute`, and `Transaction` derefs to `Connection`, so
   the executes port over unchanged to `conn.execute`) → `execute_batch("COMMIT")`.
   On a retryable error at COMMIT (`Write-write conflict` / `Busy` / `BusySnapshot`)
   → `execute_batch("ROLLBACK")` + bounded backoff retry. Body must be re-runnable
   (`AsyncFnMut`) and must NOT consume owned data needed across retries.
3. **Relax `write_guard`** off the BEGIN CONCURRENT path so different-table commits
   run concurrently. The metadata-pointer CAS still fail-closes genuine same-table
   races: a retried same-table conflict re-reads the winner's snapshot, the
   conditional UPDATE guard mismatches (`updated_rows==0`) → existing `Conflict`
   (409). So bounded retry converges correctly; cap retries to avoid livelock.
4. FW-16 test: N concurrent `commit_table` to DIFFERENT tables on a file-backed
   store all succeed (no `database is locked`); concurrent commits to the SAME table
   → exactly one winner + one `Conflict`. (The spike's `probe` is the seed for this.)
6. **Turso MVCC concurrent writes** (the user's explicit request — see full spec in
   "Turso MVCC" section below). Do this AFTER step 5 so it lands on the `write_txn`
   helper.
7. ✅ **DONE — remaining crates refactored** (behavior-preserving, this session;
   H2 now resolved so sail's feature paths build). Every inline `#[cfg(test)] mod
   tests` moved to `tests.rs`; feature-gated integration modules extracted to
   directory modules + their own test files:
   - `lakecat-sail`: `lib.rs` **6,388 → 15 lines** (re-export header + 2 mod decls);
     `catalog_provider/{mod 1472, tests 1098}`, `sail_integration/{mod 2283,
     tests 1434}`.
   - `lakecat-graph`: `lib.rs` 1,443 → 415; `tests.rs` 266; `grust_integration/
     {mod 72, tests 680}`.
   - `lakecat-security`: `lib.rs` 2,219 → 986; `tests.rs` 983;
     `typesec_integration/{mod 98, tests 128}`.
   - `lakecat-querygraph` (`lib.rs` 1,576 + `tests.rs` 816), `lakecat-lineage`
     (497 + 521), `lakecat-api` (790 + 60): trailing test extraction.
   - `lakecat-core` left as-is (small; 254-line `lib.rs` + 356-line `sail.rs` with a
     31-line inline test — not a monolith).
   **No monsters remain** (largest prod file is `sail_integration/mod.rs` 2,283,
   down from a 58k/31k/16k/6.4k era). `#[test]` counts preserved (api 3, lineage 6,
   querygraph 13, graph 35, security 25, sail 29); default +
   `grust-local`/`typesec-local`/`sail-local`/`catalog-provider`/`--all-features`
   green; fmt clean. *Optional future polish:* sub-split `sail_integration/mod.rs`
   (2.3k) and `querygraph/lib.rs` (1.6k) by responsibility.
8. **Final verification + docs:** `cargo fmt --all -- --check`; `cargo test
   --workspace` (and `--all-features` once H2 resolved); run the repo contract
   checks in `scripts/` (`check-release-readiness.sh`, `check-release-version/proof/
   book-artifact/local-dependency/workflow-trigger-contract.sh`); `git diff --check`.
   Update this CHECKPOINT + the per-crate notes to reflect the final structure.
   Update `CHANGELOG.md`. Then the commit/branch + release-proof-refresh decision.

### H2 decision (BLOCKS `--all-features` + the `sail` feature paths)
`lakecat-sail` `sail-local`/`catalog-provider` need symbols ABSENT from `../sail`
HEAD `d66a2676` (fork commit `fdb3b657 "Expose Iceberg planning result helpers"`
isn't an ancestor; `CatalogProvider` trait was redesigned to a lakehouse API).
**Do not edit `../sail` without the user.** Options: (1) restore the seam in
`../sail` (re-apply `fdb3b657`, make `models` public, re-add trait methods) — or
port `lakecat-sail` to the new `commit_lakehouse_table`/`plan_lakehouse_scan` API
(a behavior port, not a resync); (2) pin sail to a commit that still has the
symbols; (3) defer/disable the two features. Until resolved, verify the refactor on
default + non-sail features only.

### Behavior-preserving refactor technique (apply consistently)
- New module `m`: `mod m;` at crate root + `pub(crate) use m::*;` (use plain `pub
  use` for items that were `pub` at root, to keep the PUBLIC API identical). This
  keeps every name resolvable from the crate root so the test module and `main.rs`
  don't need path rewrites. Mark moved items `pub(crate)` where the compiler flags
  private cross-module access.
- Tests: replace inline `#[cfg(test)] mod tests { … }` with `#[cfg(test)] mod
  tests;` → `src/tests/` submodule tree grouped by topic; each file uses `use
  crate::*;` (+ `use super::common::*;` for shared helpers). **Preserve the exact
  `#[test]` count** (grep before/after).
- Per-crate gates only (`cargo check/test -p <crate>`), not `--all-features`
  (blocked by H2). Confirm the default-run test count matches baseline.
- Known pre-existing finding to log, not fix: `cli` `tests/lineage.rs` is a single
  **4,311-line `#[test] fn`** (`qglake_lineage_drain_verifier_requires_delivered_
  events`) — unsplittable without changing the test; future test-quality work.

### Turso MVCC — full implementation spec (user request; pinned `turso 0.7.0-pre.10`)
Mechanism is confirmed present in the pinned dep — **no version bump**:
- Enable via **`PRAGMA journal_mode = mvcc`** (replaces `journal_mode=WAL`;
  `turso_core/vdbe/vacuum.rs:562` does exactly this; the `JournalMode` opcode sets
  the `MvStore` at runtime per `vdbe/execute.rs:593`). Set it (and `busy_timeout`)
  on **every** connection in `connect()` + `migrate()` — fixes finding **I2**.
- Turso MVCC = snapshot isolation with **EAGER write-write conflict detection**
  (`mvcc/database/hermitage_tests.rs:16-27`): conflicts fail immediately at write
  time with `LimboError::WriteWriteConflict` (also `Busy`/`BusySnapshot`), surfaced
  through the binding as `turso::Error`.
- **Relax the global `write_guard()` mutex** (finding **L3**) so commits to
  different tables/warehouses run concurrently. Wrap each write transaction (the new
  `write_txn` helper from step 5) in a **bounded retry+backoff** that retries on
  `WriteWriteConflict`/`Busy`/`BusySnapshot`. Genuine logical commit races still
  resolve to the existing metadata-pointer CAS `Conflict` (409) — keep that.
- `BEGIN CONCURRENT TRANSACTION` (`TransactionType::Concurrent`) exists in the
  parser but the binding's typed tx API only exposes Deferred/Immediate/Exclusive.
  **Open question to resolve empirically:** whether ordinary `conn.transaction()`
  under `journal_mode=mvcc` already gives concurrency, or whether raw `BEGIN
  CONCURRENT` is required (then issue it via `execute_batch`).
- **Add the multi-thread concurrency test** (finding **FW-16**): N concurrent
  `commit_table` tasks to DIFFERENT tables on a file-backed store complete without
  "database is locked"; concurrent commits to the SAME table yield exactly one
  winner + one `Conflict`. This is the proof the change works.

---

## ⚠️ Build & verification status (as of 2026-06-25 review)

> Superseded by the SESSION CHECKPOINT above for live state. Kept as the review-time
> snapshot (maps to issues H2/H3).

Authoritative, captured via real `cargo` builds against the checked-out siblings:

| Build | Result |
|---|---|
| `cargo check --workspace` (default features) | ✅ **PASS** (exit 0). One warning only: unused imports `crates/lakecat-sail/src/lib.rs:11`. |
| `cargo check --workspace --all-features` | ❌ **FAIL** (exit 101) — `lakecat-cli` does not compile under `qglake-fixture`. |
| `cargo build -p lakecat-sail --features sail-local` | ❌ **FAIL** — sibling API drift (E0432/E0603). |
| `cargo build -p lakecat-sail --features catalog-provider` | ❌ **FAIL** — sibling API drift (E0432/E0407). |
| `cargo fmt -p lakecat-sail -- --check` | ❌ drift (committed). Other crates clean. |

So **three feature builds are red**; the "default build passes" claim is true but
hollow for `lakecat-sail` (~59% of that crate is behind the broken gates). These
break the stated `cargo test --workspace --all-features` gate in AGENTS.md.
**Fix these before relying on any all-features gate or the refactor's test runs.**

**Test inventory** (all `#[cfg(test)] mod tests` *inside* the source files, not
separate files — see refactor §): cli 500, service 477, store 183, graph 35,
sail 29, security 25, querygraph 13, lineage 6, api 3, core 1. `lakecat-core`,
`-api`, `-lineage` are thinly tested; there is **no multi-thread/cross-process
concurrency test** and **no live-HTTP CLI test**.

Note: a cold `cargo check --workspace` took ~17 min here — use `cargo check -p
<crate>` per refactor step and run full gates only at milestones.

---

## Architecture map (verified anchors)

**READ PATH:** REST handler → `request_identity()` (`lakecat-service/src/lib.rs:12914`,
header-trusted, attestation `"unverified"`) → `authorize()` (`:13117`) builds
context + calls `governance.authorize()` (context-blind →
`TypeSecGovernanceEngine.authorize` → `engine.check`, `lakecat-security/src/lib.rs:2009`)
→ store read (`load_table` `lakecat-service/src/lib.rs:9032`; records re-validated
on read `lakecat-store/src/lib.rs:8060`) → `LoadTableResponse` inlines full
metadata + `metadata_location` (`:12282`). Governed reads narrow via
`effective_projection`/`mandatory_filters` (`lakecat-security/src/lib.rs:201`).
Sail scan planning is behind `sail-local` (`DeferredSailCatalogEngine.plan_scan`
returns `NotSupported`, `lakecat-core/src/sail.rs:201`).

**COMMIT PATH:** `commit_table_in_warehouse` (`lakecat-service/src/lib.rs:10342`)
→ `state.sail.prepare_commit` (deferred passthrough on default build,
`lakecat-core/src/sail.rs:178`) → `write_planned_metadata` via `PutMode::Create`
with prefix/credential validation (`:10402`,`:10468`) → `store.commit_table`.
Store commit (`lakecat-store/src/lib.rs:8236`) runs under the serialized
`write_lock` (`:7836`) in one tx: conditional optimistic UPDATE guarded on prior
`metadata_location` (`updated_rows==0 ⇒ Conflict`, `:8253`) → `metadata_pointer_log`
→ `audit_events` → `outbox_events` → `idempotency_records` → `tx.commit`.
Staged-metadata cleanup only on `Err` (`lakecat-service/src/lib.rs:10662`).

**DURABLE SPINE / OUTBOX:** `lakecat-store` single ~16.9k-line `lib.rs`.
`CatalogStore` trait (`:17`,`:70`); `MemoryCatalogStore` (reference) +
`TursoCatalogStore` (`turso-local`). `write_lock` = `Arc<tokio::Mutex<()>>`
(`:7842`); all 15 writers acquire it. Outbox events staged **in the same tx** as
the catalog mutation; `.emit()` only at drain. `drain_outbox_once`
(`lakecat-service/src/lib.rs:1199`) projects-then-acks all-or-retry.
**No background drain driver** in `main.rs` — projection only advances via
`POST /management/v1/lineage/drain` (`:11279`).

**SIBLING WIRING:** `lakecat-sail` two feature-gated modules — `catalog_provider`
(`catalog-provider`) gates every op through governance before delegating to Sail
(no raw creds, `storage_credentials: None`); `sail_integration` (`sail-local`)
does commit-requirement validation / scan planning / manifest expansion / v4 JSON
bridge. `lakecat-security`: thin TypeSec boundary → `typesec::RbacEngine`/
`ComposedEngine`, fail-closed verdict mapping. `lakecat-graph`/`-lineage`:
sink boundaries delegating to `grust_graph` / emitting OpenLineage 2-0-2.
`lakecat-querygraph`: content-addressed bundle + verifiable manifest;
receipt-chain logic stays in service.

**DEFAULT-BUILD DEFAULTS (conservative):** `MemoryCatalogStore` +
`AllowAllGovernanceEngine` + `ConservativeCredentialIssuer` (public-only) +
`NoopCatalogGraphSink` + `ConservativeTypeDidVerifier` (rejects envelopes)
(`lakecat-service/src/lib.rs:968`, `main.rs:90`). Service binds **`127.0.0.1:8181`**
(`main.rs:50`).

---

## Per-crate notes

| Crate | LoC | Role / state |
|---|---|---|
| `lakecat-core` | ~0.6k | Types/traits/errors/validation/content-hash + `SailCatalogEngine` seam. Thin, clean. `LakeCatError` coarse (5 variants); `initial_table_metadata` minimal v2 stop-gap; deferred `prepare_commit` passes updates through unvalidated. |
| `lakecat-api` | ~0.85k | Iceberg REST wire models. Map fields are `Vec<ConfigEntry>` → serialize as **arrays** (spec break). No `ErrorModel`. `CommitTableRequest` drops spec `identifier`, adds non-standard fields. |
| `lakecat-store` | ~16.9k | Durable Turso spine + `MemoryCatalogStore` + migrations + atomic outbox. **Strong.** Coarse global write lock; cross-backend divergences; `busy_timeout` only on migrate conn; no concurrency test. |
| `lakecat-service` | ~58.5k | Orchestration: read/commit/handlers/outbox/wiring. Strongest commit path. Most security findings cluster here (identity trust root, raw-cred exception, allow-all wiring); default-build commit drops updates; createTable doesn't persist metadata object; no background drain. **Near-monolithic.** |
| `lakecat-sail` | ~6.4k | Iceberg v3→v4 JSON bridge + sail provider + plan-task signing. Governance/credential boundary respected (no raw vending). **`sail-local` + `catalog-provider` don't compile.** Local reimpls (pruning, type conv, snapshot chaining); lossy decimal/timestamptz. |
| `lakecat-security` | ~2.2k | TypeSec boundary — delegates, no RBAC reimpl, fail-closed. Engine invoked **context-blind**; allow-all reachable by default config & labeled `engine=typesec`; ~336 LoC ODRL parsing reimplements `typesec-odrl` (latent, no live caller). |
| `lakecat-graph` | ~1.4k | Catalog graph sink → Grust. Disciplined. Dead taxonomy variants reach into Sail's domain; hash determinism depends on `serde_json` `preserve_order` off. |
| `lakecat-lineage` | ~1.0k | OpenLineage sink/outbox + content-hash receipts. Disciplined. Hashing not RFC-8785 canonical; unhashable-event runId fallback collapses ids. |
| `lakecat-querygraph` | ~2.4k | QG bootstrap projection (Croissant/CDIF/OSI/ODRL/OpenLineage) + receipt-chain. Near-pure projection. Multi-warehouse dangling edges (no live caller); namespace-id aliasing; non-canonical hashing. |
| `lakecat-cli` | ~31.3k | HTTP client + offline JSON verifier; hand-rolled dispatch. **`qglake-fixture` does NOT compile.** 31k-line single `main.rs` w/ 500+-line fns; raw (unencoded) URL segments; no live-HTTP test. |

---

## Strengths (intentionally solid — change with care)

1. **Transactional outbox is truly atomic** — outbox/audit/pointer-log/idempotency
   rows share one tx with the catalog UPDATE (`lakecat-store/src/lib.rs:8236`).
2. **Turso write-serialization fix is correct & complete** — per-store async
   `write_lock` guards all 15 writers, reads unguarded, no reentrant double-lock,
   SQL CAS keeps races fail-closed as `Conflict` (`:7836`,`:8253`; tested `:14761`).
3. **Feature-gate intent is honest** — default features empty; deferred engine
   fails closed (`NotSupported`) for scan/fetch rather than faking empty plans.
4. **Credential vending is well-governed** — blocked reads vend zero creds with a
   recorded reason; audit stores only hashes/counts (`:9192`).
5. **No `unwrap`/`expect`/`panic`/`todo!`/`unimplemented!` on any real code path**
   — only inside `#[cfg(test)]` or on infallible literals.
6. **Redaction discipline** — metadata locations SHA-256-hashed in errors; storage
   profiles reject embedded raw secrets on read & write.
7. **Boundary discipline is mostly genuine** — security→typesec, graph→grust,
   querygraph→QueryGraph; sail provider never vends raw creds.
8. **Conservative-by-construction default-build defaults** (see arch map).

> The map/Street-View-equivalent here is the **commit path + Turso CAS + outbox**:
> these are correct and load-bearing. Refactor *around* them; don't alter their
> semantics.

---

## Findings (full list; severity · status from adversarial verify)

Status: `confirmed` / `partial` (partially-confirmed) / `unverified`
(low/info, no adversarial pass) / refuted items were dropped.

### HIGH

- **H1 · consistency · confirmed** — Cited release proof `72df4eed` invalidated by
  28 later commits (+ 4 stale derived SHA-256 hashes). `docs/book/lakecat.md:1633`,
  `:1639`; enforced by `scripts/check-release-proof-contract.sh:37`.
- **H2 · correctness · confirmed** — `sail-local` **and** `catalog-provider` builds
  fail to compile (sibling API drift). `lakecat-sail/src/lib.rs:2630` (sail-local),
  `:29`,`:557`,`:617` (catalog-provider). E0432/E0603/E0407 vs `../sail`.
- **H3 · build/feature-gate · confirmed** *(review missed this; added by build
  evidence)* — `lakecat-cli` does not compile under `qglake-fixture` /
  `--all-features`. Stale `CreateTableRequest` literal in `ensure_qglake_table`
  (`lakecat-cli/src/main.rs:7262`): passes `location` as `String` (now
  `Option<String>`, E0308 `:7264`) and omits `schema`,`partition_spec`,
  `write_order`,`properties`,`stage_create` (E0063). Fallout from spec-conformant
  createTable (commit `ad14425c`). Fix: wrap `Some(...)` + add the 5 `None` fields.
- **H4 · correctness · partial** — createTable returns a `metadata_location` it
  never writes to storage (`write_planned_metadata` only runs from commit). Table
  still loads (metadata inlined) but the location 404s until first commit.
  `lakecat-service/src/lib.rs:8952` vs `:10402`.
- **H5 · docs · confirmed** — Book onboarding uses port **3000**; service binds
  **8181** → every curl/Spark example fails connection-refused.
  `main.rs:50` vs `docs/book/lakecat.md:724,756,866,909` (+11 more).
- **H6 · security · confirmed** — Bare `x-lakecat-principal` defaults to
  `PrincipalKind::Human` with no verification → trivial impersonation +
  self-asserted raw-credential bypass. `lakecat-service/src/lib.rs:12914`,`:13166`,
  `:9206`. Contained on default build; exploitable with `typesec-local` + no policy.
- **H7 · security · confirmed** — Raw-credential exception decided by lakecat's
  `principal.kind==Human` heuristic, **not** re-evaluated by the TypeSec engine
  (no distinct raw-vs-governed action). `:13166`,`:9206`; `lakecat-security:2001`.
- **H8 · spec-conformance · confirmed** — Map-typed Iceberg REST fields serialized
  as **JSON arrays** (`Vec<ConfigEntry>`), breaking stock pyiceberg/Spark/Trino on
  the bootstrap `/config` call. `lakecat-api/src/lib.rs:196,247,20-21,332,110`.
- **H9 · spec-conformance · confirmed** — Default-build commit silently drops REST
  `updates` and skips `requirements` validation (returns 200, table unchanged;
  assert-ref/assert-create ignored). `lakecat-core/src/sail.rs:178`.

### MEDIUM

- **M1 · security · confirmed** — `typesec-local` without `LAKECAT_TYPESEC_RBAC_POLICY`
  silently wires allow-all governance **+ a real secret-ref resolver** — the exact
  config a first-time operator reaches for. `lakecat-service/src/main.rs:95`,
  `lib.rs:208`,`:314`.
- **M2 · security · confirmed** — typesec allow-all path reports `engine="typesec"`
  with a synthetic `policy_hash`, indistinguishable from an enforced allow (the
  non-typesec path uses a distinct honesty label). `lakecat-security/src/lib.rs:2016`.
- **M3 · security · confirmed** — Plan-task HMAC bypassable: `decode_plan_task` also
  accepts unsigned `lakecat:sail-json:` and plain forms with no signature check.
  Bounded by downstream re-validation. `lakecat-sail/src/lib.rs:4428`,`:3551`.
- **M4 · spec-conformance · confirmed** — createTable auto-creates a missing
  namespace (`insert or ignore`) instead of 404 `NoSuchNamespace`.
  `lakecat-store/src/lib.rs:8078`,`:1915`.
- **M5 · spec-conformance · confirmed** — `create_namespace` hides AlreadyExists
  (`insert-or-ignore` ⇒ 200 not 409). Both backends. `lakecat-store/src/lib.rs:7916`.
- **M6 · spec-conformance · confirmed** — Iceberg `ErrorModel` returns opaque
  `type="LakeCatError"` for all errors; no `ErrorModel` in `lakecat-api`.
  `LakeCatError` too coarse (Conflict collapses already-exists/commit-conflict/
  authz-denied; authz returns 409 not 401/403). `lakecat-service/src/lib.rs:13417`.
- **M7 · spec-conformance · confirmed** — v4-extension commit-requirement validation
  silently passes unchecked non-main ref assertions (+ ignores unknown requirement
  types) → weakens optimistic concurrency. `lakecat-sail/src/lib.rs:3215`,`:3267`.
- **M8 · boundary · partial** — TypeSec engine invoked **context-blind** (`check`,
  not `check_with_context`); `request.context` reaches only `policy_hash`, not the
  decision. Latent today (only allow-all/RBAC wired). `lakecat-security:2009`.
- **M9 · boundary · confirmed** *(added by critic)* — ~336 LoC of ODRL
  read-restriction parsing reimplemented in the catalog instead of using TypeSec's
  `typesec-odrl` crate (zero references). Latent/dead-but-tested (no live caller).
  `lakecat-security/src/lib.rs:121-457`.
- **M10 · boundary · partial** — File-level metrics pruning (~220 LoC) hand-rolled
  instead of delegated to Sail's `prune_files`. Impedance mismatch (JSON vs
  DataFusion Expr) blocks a free swap. `lakecat-sail/src/lib.rs:3857`.
- **M11 · correctness · confirmed** — Iceberg→DataFusion type conversion drops
  decimal `(P,S)` (hardcodes `Decimal128(38,18)`) and timestamptz tz. Fallback path
  only; under `catalog-provider` (which doesn't compile). `lakecat-sail:1223`,`:1248`.
- **M12 · maintainability · confirmed** — 31k-line single `main.rs` with 500+-line
  fns (CLI). `lakecat-cli/src/main.rs:1`, `:3152` (~537-line fn). → refactor §.
- **M13 · docs · confirmed** — Book release prose frozen at 0.1.0/Unreleased while
  repo is 0.1.1. `docs/book/lakecat.md:1595,1694` vs `Cargo.toml:18`.
- **M14 · docs · confirmed** — Stale release proof: post-`72df4eed` executable
  commits have no fresh gate/CHANGELOG entries. `STATUS.md:19`, `CHANGELOG.md`.
- **M15 · docs · confirmed** — Turso write-serialization fix undocumented in the
  book's durable-spine chapter. `docs/book/lakecat.md:496` vs `store:7836`.

### LOW

| ID | cat · status | Finding | Location |
|---|---|---|---|
| L1 | boundary · partial | Control-plane keys (`lakecat:version`, `lakecat:last-request-hash`) injected as **top-level** Iceberg metadata fields (belongs in `properties`). Rejection risk hypothetical. | `lakecat-store/src/lib.rs:2030`,`:8250` |
| L2 | boundary · unverified | Dead graph taxonomy variants (Manifest/DataFile/DeleteFile) reach into Sail's domain; never produced. | `lakecat-graph/src/lib.rs:333` |
| L3 | concurrency · unverified | Global per-store write `Mutex` serializes writes across unrelated warehouses; couples outbox relay to commit path. Relaxable once Turso ships MVCC. | `lakecat-store/src/lib.rs:7842` |
| L4 | concurrency · unverified | Table-creation audit/outbox not transactionally paired with `create_table` (commit path is). Crash ⇒ table with no `created` event. | `lakecat-service/src/lib.rs:8993` |
| L5 | consistency · partial | `docs/RELEASES.md` "Released" table still lists only v0.1.0; RELEASE.md prose narrates v0.1.0. (Machine version contract passes.) | `docs/RELEASES.md:7`, `RELEASE.md:174` |
| L6 | consistency · unverified | Idempotent-replay-after-soft-delete diverges Memory (NotFound) vs Turso (replays). Turso arguably more correct. | `lakecat-store/src/lib.rs:1963` vs `:8179` |
| L7 | correctness · partial | createTable default location hardcodes `file:///tmp/lakecat/...` ignoring storage profile. Ergonomics, not the claimed breakage. | `lakecat-service/src/lib.rs:8954` |
| L8 | correctness · unverified | `validate_name` allows `.` in components/names → `['a.b','c']` and `['a','b.c']` both render `a.b.c` (namespace aliasing; querygraph dedup). | `lakecat-core/src/lib.rs:239`,`:93` |
| L9 | correctness · partial | Multi-warehouse querygraph build emits edges to warehouse nodes never created. No live caller (bootstrap is single-warehouse). | `lakecat-querygraph/src/lib.rs:976`,`:894` |
| L10 | correctness · unverified | Fragile control-flow `unwrap` in CLI view-receipt admission; could panic on attacker JSON after a refactor. Match on `Option` instead. | `lakecat-cli/src/main.rs:5844` |
| L11 | security · partial | Hardcoded default plan-task signing key fallback, no warning/doc. Not the access gate (re-validation gates). Fail-closed-or-warn. | `lakecat-sail/src/lib.rs:2658`,`:4539` |
| L12 | spec · unverified | CLI interpolates URL path segments raw (`namespace.join('.')`), no percent-encoding → malformed/traversing URLs on default-build commands. | `lakecat-cli/src/main.rs:99,137,7028,7128` |
| L13 | spec · unverified | Content hashing deterministic but **not** RFC-8785/JCS canonical → cross-language importer hazard (qg-rust must byte-match serde_json). Silent break if `preserve_order` ever enabled. | `lakecat-core/src/lib.rs:228` |
| L14 | spec · partial | `CommitTableRequest` omits spec `identifier`, adds non-standard `metadata`/`metadata_location`; no `deny_unknown_fields`. Non-breaking for endpoints implemented. | `lakecat-api/src/lib.rs:706` |
| L15 | test-coverage · unverified | `validate_fetch_tasks_shape` substitutes empty Vec when any plan-tasks exist → never validates plan-task content. | `lakecat-sail/src/lib.rs:4147` |

### INFO

- **I1 · consistency** — Duplicate audit-event id error shape diverges Memory vs
  Turso (both `Internal`, different messages). `lakecat-store/src/lib.rs:9370` vs `:2760`.
- **I2 · correctness** — `busy_timeout`/WAL pragmas set only on the `migrate()`
  connection (errors swallowed), not per-operation connections, so the timeout
  doesn't govern real read/write conns. `lakecat-store/src/lib.rs:7884`.

---

## Future work (consolidated · priority · doc-stated vs inferred)

### High priority
- **FW-1 (inferred)** — Make default-build commit **reject** updates/requirements
  it can't apply (mirror plan_scan's `NotSupported`) instead of silently accepting.
  `lakecat-core/src/sail.rs:178`. *(fixes H9)*
- **FW-2 (doc-stated, DESIGN.md OPUS1 F2)** — Verify principal identity before
  granting trusted-human / raw-credential privileges; don't default bare
  `x-lakecat-principal` to Human, or require an authenticating proxy + document it.
  *(fixes H6/H7)*
- **FW-3 (doc-stated, DESIGN.md F10)** — Re-sync `lakecat-sail` to current sibling
  APIs, pin sibling commits, and **add a feature-matrix build gate** that actually
  builds `sail-local`/`catalog-provider`/`qglake-fixture`. *(fixes H2/H3)*
- **FW-4 (inferred)** — Serialize Iceberg-spec map fields as JSON **objects**
  (`BTreeMap`/map adapter) + round-trip object-shape tests. *(fixes H8)*
- **FW-5 (doc-stated)** — Refresh the release-candidate proof; add CHANGELOG
  entries for post-`72df4eed` commits; reconcile book/version/release docs to
  0.1.1; tag v0.1.1 after a fresh full gate. *(fixes H1/M13/M14)*
- **FW-6 (inferred)** — Fix book onboarding port (3000→8181) or document
  `LAKECAT_BIND_ADDR`. *(fixes H5)*

### Medium priority
- **FW-7** — Write the synthesized initial metadata object on createTable (reuse
  `write_planned_metadata`, cleanup on failure); derive default location from the
  storage profile. *(fixes H4/L7)*
- **FW-8** — Map `LakeCatError` → Iceberg exception types; define
  `ErrorModel`/`IcebergErrorResponse` in `lakecat-api`; add Unauthorized/Forbidden/
  CommitConflict/AlreadyExists variants. *(fixes M6)*
- **FW-9** — Require an explicit policy (or named demo flag) when `typesec-local` is
  enabled; give the allow-all path a distinct receipt engine label. *(fixes M1/M2)*
- **FW-10 (doc-stated)** — Push the raw-vs-governed credential distinction into the
  TypeSec engine as a distinct action/attribute (`credentials.vend-raw`). *(fixes H7)*
- **FW-11** — Return 404 NoSuchNamespace on createTable; 409 on duplicate
  create_namespace. *(fixes M4/M5)*
- **FW-12** — Reject unsigned plan-task tokens; fail-closed/warn on default signing
  key. *(fixes M3/L11)*
- **FW-13 (doc-stated, DESIGN.md F5)** — Delegate file/manifest pruning + snapshot
  chaining to Sail (or have Sail expose a REST-JSON pruning entrypoint). *(fixes M10)*
- **FW-14** — Fix typed-schema conversion (decimal `(P,S)`, timestamptz UTC); prefer
  Sail's typed schema. *(fixes M11)*
- **FW-15** — Validate non-main ref assertions + reject unknown requirement types in
  the v4-extension commit path. *(fixes M7)*
- **FW-16** — Add concurrency (multi-thread `commit_table`), cross-backend, and
  live-HTTP CLI test coverage; tenant `from_records` end-to-end test.
- **FW-17** — Spawn a background outbox drain task (with backoff + toggle) **or**
  document the operator-polling contract for `/management/v1/lineage/drain`.
- **FW-18** — Adopt canonical JSON (JCS/RFC-8785) for content hashing **or** pin a
  golden-hash fixture shared with qg-rust; add a guard that `preserve_order` stays
  off. *(fixes L13)*
- **FW-19** — Disallow/escape `.` in namespace components & table names (Iceberg
  uses 0x1F unit-separator); reject empty/separator-only components. *(fixes L8)*
- **FW-20 (boundary)** — Either reference `typesec-odrl` or document the local ODRL
  parsing as an intentional bridge. *(fixes M9)*

### Low priority
- **FW-21** — Percent-encode CLI URL path segments; Iceberg multipart-namespace
  encoding. *(L12)*
- **FW-22** — Set `busy_timeout`/WAL on every operational connection. *(I2)*
- **FW-23** — Emit `table.created` audit/outbox inside `create_table`'s tx. *(L4)*
- **FW-24** — Align idempotent-replay-after-soft-delete + duplicate-audit-id across
  backends. *(L6/I1)*
- **FW-25** — Make the CLI receipt-admission `unwrap` fail-closed; propagate the
  lineage unhashable-event error instead of a colliding runId. *(L10)*
- **FW-26** — Remove or doc-mark dead graph taxonomy variants. *(L2)*
- **FW-27 (doc-stated, GOAL.md/DESIGN.md)** — Keep pushing reusable read-execution /
  typed Iceberg v4 / view-history into Sail; keep v4 JSON passthrough an explicit
  bridge only.
- **FW-28 (doc-stated)** — Move remaining side effects to the outbox; refresh
  qg-rust dep-guide examples to Grust 0.10.0 / TypeSec 0.8.0; replace temporary Sail
  helper bridges once upstream publishes; add cloud-SDK secret resolvers.
- **FW-29 (doc-stated)** — Resolve & document the publishable-vs-functional stance
  for `lakecat-service` (path deps via `sail-local` block crates.io); bump intra-
  crate version pins to 0.1.1 (api/store/sail still pin 0.1.0).
- **FW-30 (doc-stated, `docs/RELEASES.md:23`)** — Planned SemVer/codename roadmap
  0.2 Lynx → … → 1.0 Lion.
- **FW-31 (doc-stated, `docs/book/lakecat.md:1282`)** — Possible optional
  "catalog-reliability / event-admission" profile (upstream proof-envelope concepts).
- **FW-32 (doc-stated, `docs/book/lakecat.md:1878`)** — Prove the bootstrap bundle
  through QueryGraph import on every meaningful public-surface change (keep local
  release evidence ahead of cloud CI).

---

## Open questions (maintainer decisions — several findings hinge on these)

1. **Stock-client interop?** Is LakeCat meant to serve stock Iceberg REST clients
   (pyiceberg/Spark/Trino) or only a LakeCat-aware client? Array-vs-object
   serialization (H8), bespoke view model, and control-plane keys in metadata (L1)
   are only acceptable in the latter case — but AGENTS.md says "don't fork Iceberg."
2. **Default-build updateTable scope?** Should the no-`sail-local` build accept
   Iceberg `updateTable` at all, or only register-style full-metadata commits?
   Determines whether H9 is a bug or needs an explicit reject. *(drives FW-1)*
3. **Is bare `x-lakecat-principal`→Human intentional** (catalog always behind a
   trusted authenticating proxy), or a code defect? *(drives FW-2; H6/H7)*
4. Should the TypeSec engine receive the raw-credential exception as a distinct
   action, and should `typesec-local`+no-policy **fail closed**? *(M1/FW-9/FW-10)*
5. Which sibling commit of `../sail` is `lakecat-sail` meant to target, and where's
   the CI step that builds the feature gates? *(H2/H3/FW-3)*
6. Should createTable persist initial `metadata.json` eagerly, or is the contract
   "location only resolves after first commit"? The new test asserts neither. *(H4)*
7. Does qg-rust validate `bundle_hash`/`view_receipt_evidence_hash` and reproduce
   `content_hash_json` by byte-matching serde_json or expect JCS? Golden fixture?
   *(L13/FW-18)*
8. Should namespace components allow `.` at all? *(L8/FW-19)*
9. Is manual-only CI the intended 0.1.x posture, and is allow-all + real-secret
   resolver strictly for demos? *(M1)*

---

## Human-reviewability refactor (in progress — requested 2026-06-25)

Goal: no monstrous monolithic files, DRY / clear reuse, **tests in separate
files**. Behavior-preserving (pure structure), verified by existing tests at each
step. Current shape is near-single-file crates with `#[cfg(test)] mod tests`
inline (service ~58.5k, cli ~31.3k, store ~16.9k).

**Sequencing (worst monoliths first; gate green per crate):**
0. **Prerequisite — make the gates green:** fix H3 (cli `qglake-fixture`
   `CreateTableRequest`), then H2 (`lakecat-sail` sibling drift) so
   `--all-features` tests can run. Also `cargo fmt -p lakecat-sail`.
1. `lakecat-service` → modules by responsibility (identity/auth, read path, commit
   path, handlers, outbox/drain, sail/grust/typesec wiring, error envelope, types).
2. `lakecat-cli` → `cli/{args, http, verify_handoff, verify_replay, fixture}`
   (fixture feature-gated); break the 350–537-line fns.
3. `lakecat-store` → `{store trait, memory, turso, migrations, outbox, audit}`.
4. Then `lakecat-sail`, and the small crates as needed.
5. **Move `#[cfg(test)] mod tests` out** into sibling `tests.rs`/`tests/`
   per module; extract duplicated helpers (DRY).

**Gate per crate:** `cargo build -p <crate>` + the focused `cargo test` from
AGENTS.md + `cargo fmt`. Full `cargo test --workspace --all-features` at
milestones (only after step 0).

**Release-proof caveat:** the repo records release proof from clean head
`72df4eed` and treats local evidence as authoritative. This refactor moves that
head; keep `CHANGELOG.md` updated per logical unit (AGENTS.md convention) and
**confirm with the maintainer before committing / re-running release proof**.

---

## How this review was produced (coverage caveats)

Multi-agent review (2026-06-25): 12 parallel reviewers over the book, project
docs, and all 10 crates (two lenses on `lakecat-service`) → adversarial verify of
each gap/bug finding → synthesis → completeness critic. ~53 agents. Default-feature
builds verified exit 0; the 3 failing feature builds reproduced with exact error
codes. The Turso fix / outbox atomicity / CAS were audited method-by-method (all 15
writers) and corroborated by git (`934162f1`). **Sampled, not fully read:**
`STATUS.md` (~1 MB / 17.6k lines) and `CHANGELOG.md` (~381 KB) — themselves a doc-
hygiene concern. Line numbers are reviewer-cited (a few had off-by-prefix issues,
corrected where verdicts noted). Refuted findings (e.g. "createTable breaks first
commit under non-/tmp profile", "v4 acceptance contradicts docs", "Vault client
leaks secret-ref URL") were dropped.
