# LakeCat Goal

This is the live post-foundation goal. The original LakeCat charter is preserved
unchanged in `docs/completed/GOAL1.md`.

## Objective

Prepare and sustain LakeCat as a release-ready Rust-native, Iceberg REST
catalog foundation for QueryGraph. Keep the catalog boundary thin, preserve
standard client compatibility, and move reusable table semantics to Sail,
graph behavior to Grust, and governance semantics to TypeSec.

## Current Stage

- The first-release local candidate proof is recorded from clean head
  `72df4eed`; see `RELEASE.md`, `STATUS.md`, and `CHANGELOG.md`.
- Do not rebuild tracked book artifacts unless deliberately finishing a release.
  Keep `docs/book/lakecat.md` current as behavior and workflows change.
- Keep CI manual-only. Local release evidence is authoritative.
- Current LakeCat dependencies are Grust `0.10.0` and published TypeSec `0.8.0`.
  QueryGraph's live `qg-rust` importer matches those versions and LakeCat's
  receipt-chain contract; refresh its stale dependency-guide examples before
  QueryGraph's next public release.

## Next Stage

1. Keep release proof fresh after executable changes with the full local gate.
2. Replace temporary Sail helper bridges only when upstream helpers are
   published and covered by Sail tests.
3. Keep v4 JSON bridging explicit. Apache Iceberg v4 remains a draft; typed
   metadata, relative-location, manifest, delete, and planning support belongs
   in Sail after formal specification adoption.
4. Keep QueryGraph QGLake verify/import as the end-to-end acceptance boundary.
5. Continue to use the repo boundaries and verification discipline in
   `AGENTS.md` as binding guidance.

## Source Of Truth

Read `AGENTS.md`, `DESIGN.md`, `STATUS.md`, `ARCHITECTURE.md`, `RELEASE.md`,
the LakeCat book, and the live code before selecting work. Historical goals and
OPUS documents under `docs/completed/` are audit records, not active plans.
