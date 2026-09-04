# LakeCat ↔ Sail integration

This is the canonical reference for how LakeCat consumes **Sail** — what the
dependency is, what the shared branch carries, where the seam between the two
repositories sits, and how to move the pin forward. It is the engineering
companion to the architecture story in `docs/book/lakecat.md` ("The Siblings and
the Engine Path", "The Benchmark Suite") and to the agent-facing notes in
`CLAUDE.md` and `AGENTS.md`. It does not restate those; where they already cover
something, it links rather than duplicates.

The one-sentence frame from `AGENTS.md` holds: **LakeCat owns the catalog boundary
— identity, tenancy, Iceberg REST compatibility, metadata-pointer state, policy
gates, integration events — and pushes everything that needs deep table-format
knowledge into Sail.** This document is about the wire between those two halves.

---

## Why Sail is a git dependency on the `lakecat` branch

LakeCat consumes Sail as a **Cargo git dependency on the `lakecat` branch of
`https://github.com/querygraph/sail`** (public). It is not a local `../sail` path
dependency, and there is no `ci/sail-patches` bridge — both were retired when the
branch moved under the querygraph org.

The full rationale lives in `CLAUDE.md` ("🔗 Sail dependency"); the short version:

- **Not a path dep**, because the build must be fetchable by Cargo with no Sail
  checkout present — locally or in CI. `Cargo.lock` pins the exact
  `git+…?branch=lakecat#<sha>` rev, so every build resolves to one Sail commit.
- **Not only upstream Sail**, because LakeCat still needs catalog-provider and
  commit-update seams that are specific to its integration timeline. Reusable
  performance work is being proposed upstream in
  [`lakehq/sail#2400`](https://github.com/lakehq/sail/pull/2400); the
  `querygraph/sail` `lakecat` branch keeps LakeCat's additional APIs available
  while that review proceeds.
- The branch is meant to **shrink over time**: rebase it onto `lakehq/sail` main
  periodically, and when everything LakeCat needs is upstream, point the git dep
  at `lakehq/sail` (or a published crate) and retire the branch.

### Where it is declared

`[workspace.dependencies]` in `Cargo.toml` pins `sail-catalog`,
`sail-catalog-iceberg`, `sail-common-datafusion`, and `sail-iceberg` to
`{ git = "https://github.com/querygraph/sail.git", branch = "lakecat" }`; every
crate references them via `{ workspace = true }`. `Cargo.lock` records the locked
rev.

---

## What the `lakecat` branch carries today

The branch forked from upstream after `lakehq/sail#2134` and carries the selected
LakeCat integration and performance commits that have accumulated since then. It
is not currently a rebased copy of upstream `main`; general-purpose changes are
kept patch-aligned with their upstream candidates while LakeCat-only APIs remain
on this line. As of the current pin (`querygraph/sail` `lakecat` at
**`dbff52b0dfff5fed302d09a72eeb7feb92f50725`**) it carries four groups of work:

1. **The original LakeCat-needed Sail commits** (the baseline that made the
   `sail-local` / `catalog-provider` feature builds compile and pass):
   - `apply_table_updates` (built on the merged upstream #2134 `TableUpdate`
     enums) — the entry point LakeCat's commit path calls to evolve table
     metadata.
   - the manifest `lower_bounds` / `upper_bounds` Avro round-trip fix (a genuine
     `sail-iceberg` bug — `bytes`-typed map entries were being written as Avro
     arrays and silently dropped on read).
   - the pruning type-mismatch guard.
   - the Iceberg planning / `models` exposure plus the `CatalogProvider`
     commit-table seam.

2. **The Foyer object-store cache** (addressing lakehq/sail issue **#1015** and
   proposed upstream in **#2400**) — a per-worker read-through page cache in
   Sail's `sail-object-store` crate. The QueryGraph implementation is
   semantically identical to the upstream PR, with only branch-local rustfmt
   layout differences. See "The object-store read cache" below.

3. **The snapshot-append updates** (originally developed on
   `feat/apply-table-updates-snapshots`) — `apply_table_updates` now handles
   `add-snapshot` and `set-snapshot-ref`, the two updates a data append produces.
   This is what lets a stock Iceberg client's `table.append` land as new table
   metadata under a `sail-local` LakeCat (see "Default build vs `sail-local`").

4. **Measured SQL, catalog, and Iceberg hot paths** — single-statement parsing,
   direct typed metadata deserialization, newest-first current metadata lookup,
   Avro reader reuse, indexed catalog schema relationships, streamlined delete
   matching/routing, shared immutable delete descriptors, and streaming metadata
   discovery. Each optimization is paired with a focused benchmark.

The reusable cache, SQL, and Iceberg work is organized for independent upstream
review in #2400. The LakeCat-specific provider and metadata-update seams can
graduate separately as upstream APIs converge.

---

## The seam

LakeCat talks to Sail through one trait and two feature gates.

### `SailCatalogEngine` (in `lakecat-core`)

The seam is the `SailCatalogEngine` trait in `lakecat-core` (`src/sail.rs`).
LakeCat's service code is written against this trait, never against Sail types
directly, so the default build can ship a deferred implementation and the
feature builds can ship the real one. The trait covers the questions that require
table-format knowledge — commit validation and metadata evolution, scan planning,
fetch-task re-validation, and metadata-as-data views. The catalog binds its proof
(pointer hashes, plan hashes, receipt hashes) around each call; Sail supplies the
table-format answer.

### Feature gates: `sail-local` and `catalog-provider`

Two features in `lakecat-sail` activate the real Sail integration:

- **`sail-local`** — the integration LakeCat ships and tests: commit-requirement
  validation, scan planning, manifest expansion, the v3→v4 JSON bridge, and
  metadata evolution via `apply_table_updates`.
- **`catalog-provider`** — routes catalog operations through Sail's
  `CatalogProvider` seam, gating every op through governance before delegating
  (no raw credentials; `storage_credentials: None`).

Both are off by default. Per `AGENTS.md`, default-feature tests pass with the
deferred seam, and real integration is proved only behind the explicit gates.

### Commit: `apply_table_updates`

On the commit path under `sail-local`, LakeCat owns the catalog half — CAS on the
metadata pointer, idempotency, pointer-log, audit, outbox — and hands the
table-metadata half to Sail's `apply_table_updates`. That function applies the
Iceberg `TableUpdate`s (including, now, `add-snapshot` and `set-snapshot-ref`) to
produce the new `metadata.json` that LakeCat then writes and points at. This is
the mechanism behind the proven stock-client write round-trip documented in the
book's benchmark chapter: a real `table.append` becomes a snapshot append that
Sail applies and LakeCat commits.

### The object-store read cache (`CachingObjectStore`)

Sail's `sail-object-store` crate provides a per-worker, read-through **page**
cache — `CachingObjectStore` over a `CacheConfig` — added for lakehq/sail #1015.
It is ported from lancedb/ocra (attributed in the crate), with the original Moka
backing store swapped for **Foyer**.

- **Opt-in.** `SAIL_OBJECT_STORE_CACHE` enables it. Defaults are **1 MiB** pages,
  **1 GiB** of weighted page memory, **64 MiB** of combined metadata/path-identity
  memory, and a **60 second** metadata revalidation TTL.
- **Configuration.** `SAIL_OBJECT_STORE_CACHE_PAGE_SIZE`,
  `SAIL_OBJECT_STORE_CACHE_MEMORY`, `SAIL_OBJECT_STORE_CACHE_METADATA`, and
  `SAIL_OBJECT_STORE_CACHE_METADATA_TTL_SECS` tune those values. A TTL of `0`
  revalidates metadata on every read.
- **Interception point.** `object_store` 0.13.2 exposes its read methods as a
  non-overridable blanket trait, so the cache cannot wrap them directly; it
  intercepts the two range entry points the engine reads through — `get_opts` and
  `get_ranges` — and serves whole pages from memory.
- **Tiering.** The current tier is in-memory only; Foyer's `HybridCache` disk
  tiering is a planned follow-up on the same seam.
- **Consistency.** Writes through the wrapper invalidate before and after the
  mutation, including multipart completion, copy, rename, and delete. External
  replacements are detected by size, modification time, ETag, and version after
  the TTL; a changed or evicted metadata identity rotates the compact object id,
  making stale pages unreachable in O(1). Conditional, version-specific, and
  backend-extension requests bypass the cache.
- **Bounded state.** Page bytes, metadata, and path identities all have weighted
  capacity bounds. Sail depends on `foyer-common` and `foyer-memory` directly, so
  enabling an in-memory cache does not pull Foyer's storage/io_uring tier into
  the build.

The cache is entirely a Sail concern — a reusable engine capability — so LakeCat
owns no cache code; it benefits through the dependency. The end-to-end benchmark
measures a warm-vs-cold scan improvement of about 26×. Production microbenchmarks
for #2400 additionally measure 1.73× faster cached 4 KiB reads, 5.42× faster
32-range batches, and 1.80× faster 16-way concurrent reads; see
`docs/book/lakecat.md` and the upstream PR description.

### Scan planning

Scan planning lives behind `sail-local`. Without it, the deferred
`SailCatalogEngine` returns `NotSupported` for scan planning rather than
fabricating an empty plan, so any real read reflects the engine that interprets
Iceberg metadata, never a catalog-shaped placeholder.

---

## Default build vs `sail-local`

LakeCat keeps the feature gate honest on the commit path, and this is the
load-bearing distinction for compatibility:

- **Default build (no `sail-local`).** The deferred seam can validate a commit but
  cannot truly apply table-metadata updates. It therefore **rejects** updates it
  cannot apply, returning `NotSupported` — the same fail-closed posture the
  deferred scan seam already takes. It does **not** silently accept and drop them.
  (This closed the earlier behavior where the default build returned `200` while
  discarding the `updates`.)
- **`sail-local` build.** Updates are really applied, through Sail's
  `apply_table_updates`, and persisted as a new `metadata.json` behind the
  metadata-pointer CAS. This is the build that carries a stock Iceberg
  write+read round-trip end to end.

The rule of thumb: the default build is conservative and fail-closed; durable
Iceberg metadata evolution is a `sail-local` capability.

---

## Bumping the Sail pin

The development loop (full version in `CLAUDE.md`):

1. Develop in a Sail checkout on the `lakecat` branch (or on a feature branch you
   then merge into `lakecat`), and push to `querygraph/sail`.
2. Advance the locked rev from LakeCat:

   ```sh
   CARGO_NET_GIT_FETCH_WITH_CLI=true \
     cargo update -p sail-catalog --precise <full-sail-commit>
   ```

   (`sail-catalog`, `sail-catalog-iceberg`, `sail-common-datafusion`,
   `sail-iceberg` are the four pinned crates.)
3. Run the focused Sail-feature tests and report them (`AGENTS.md`, Verification):

   ```sh
   cargo test -p lakecat-sail --features sail-local
   cargo test -p lakecat-sail --features catalog-provider
   ```

4. When a change touches Sail, run that repo's focused tests too and report each
   repo separately.

The `dbff52b0` alignment was validated in the shared Linux/aarch64 Docker runner
with stable Rust 1.96.0: branch formatting passed, strict object-store Clippy
passed with `--all-targets -- -D warnings`, and all 14 object-store tests passed.
The corresponding upstream #2400 run passed Rust build/tests/lint, Spark 3.5 and
4.2, Python/Spark Connect 3.5–4.2, Ibis, docs, title validation, and Codecov.

LakeCat was then validated from clean detached source commit `07635ad5` against
the exact locked Sail revision above. `lakecat-sail --features sail-local`
passed 19 tests; `lakecat-sail --features catalog-provider` passed 12; and
`lakecat-service --features sail-local` passed 464 unit tests, the compile-fail
API-authority test, and doctests. Test links used LLVM `lld` with test-only debug
information disabled because the GNU debug linker exceeded the runner's memory;
this does not alter the tested code or runtime behavior.

The production `lakecat-service` executable was built in that same container
with Rust 1.96.0, `opt-level=3`, `target-cpu=native`, thin LTO, one codegen unit,
stripped symbols, `panic=abort`, and incremental compilation disabled. Fat LTO
was attempted with both GNU ld and `lld`, but the final link was killed at the
runner's 7.8 GiB memory ceiling; thin LTO is therefore the strongest reproducible
production profile on this runner. The resulting aarch64 ELF is 20,310,656
bytes, contains no debug sections, has SHA-256
`61938db9f8cfaec4f5bace41d034c46fe0fb3312b64531b72f9d37606ac2e4f6`, and
survived a bounded startup smoke test.

**Toolchain.** Stable only — never run `cargo +nightly` (including `cargo +nightly
fmt`). Sail's CI uses nightly fmt; let Sail's CI handle it, don't run it locally.

**Branch hygiene.** LakeCat builds from the pinned Sail rev via `Cargo.lock`, so a
local Sail checkout being on a different branch does not affect a clean fetch-based
build; but if you develop against a local checkout, keep it on `lakecat` (or the
feature branch you intend to merge) so what you test matches what you pin.

---

## See also

- `CLAUDE.md` — "🔗 Sail dependency: the querygraph/sail `lakecat` branch" (the
  authoritative rationale and bump procedure; this doc links to it deliberately).
- `AGENTS.md` — repo boundaries and the verification matrix.
- `DESIGN.md` — the living design surface for the engine boundary.
- `docs/book/lakecat.md` — "The Siblings and the Engine Path" (the v3→v4 bridge
  and the LakeCat/Sail handoff table) and "The Benchmark Suite" (the object-store
  cache, rust-vs-jvm, and stock-client round-trip results).
