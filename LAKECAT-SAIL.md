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
- **Not upstream PRs**, because the Sail maintainers are actively redesigning
  catalog/table internals and asked that uncoordinated PRs wait. The
  `querygraph/sail` `lakecat` branch decouples LakeCat's velocity from the
  upstream PR timeline while keeping a single, stable, fetchable source.
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

The branch is upstream `lakehq/sail` main plus the minimal set of commits LakeCat
needs. As of the current pin (`querygraph/sail` `lakecat` at **`bddb1706`**) it
carries three groups of work:

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

2. **The Foyer object-store cache** (PR candidate branch
   `feat/object-store-foyer-cache`, addressing lakehq/sail issue **#1015**) — a
   per-worker read-through page cache in Sail's `sail-object-store` crate. See
   "The object-store read cache" below.

3. **The snapshot-append updates** (PR candidate branch
   `feat/apply-table-updates-snapshots`) — `apply_table_updates` now handles
   `add-snapshot` and `set-snapshot-ref`, the two updates a data append produces.
   This is what lets a stock Iceberg client's `table.append` land as new table
   metadata under a `sail-local` LakeCat (see "Default build vs `sail-local`").

Groups 2 and 3 were merged into `lakecat` from their PR-candidate branches; both
are written to be upstreamable to `lakehq/sail` on their own, so they can graduate
out of the `lakecat` branch independently.

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

- **Opt-in.** `SAIL_OBJECT_STORE_CACHE` enables it;
  `SAIL_OBJECT_STORE_CACHE_PAGE_SIZE`, `_MEMORY`, and `_METADATA` tune it.
  Defaults: **1 MiB** pages, **1 GiB** value memory, **64 MiB** metadata.
- **Interception point.** `object_store` 0.13.2 exposes its read methods as a
  non-overridable blanket trait, so the cache cannot wrap them directly; it
  intercepts the two range entry points the engine reads through — `get_opts` and
  `get_ranges` — and serves whole pages from memory.
- **Tiering.** The current tier is in-memory only; Foyer's `HybridCache` disk
  tiering is a planned follow-up on the same seam.

The cache is entirely a Sail concern — a reusable engine capability — so LakeCat
owns no cache code; it benefits through the dependency. The benchmark suite
measures it (warm-vs-cold scan ≈ 26×); see `docs/book/lakecat.md`.

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
   cargo update -p sail-catalog          # or the specific crate you bumped
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
