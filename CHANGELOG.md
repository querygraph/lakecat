# Changelog

## Unreleased

- Persisted commit authorization receipts into Turso audit and outbox payloads,
  keeping TypeSec governance decisions attached to the durable commit record.
- Added a Turso concurrent commit regression that exercises two writers racing on
  the same metadata pointer and verifies compare-and-swap admits only one.
- Added a typed catalog outbox drain API and Turso implementation so commit
  events can be fetched by sink and marked delivered without coupling graph or
  lineage side effects to the request path.
- Added local `file://` object-store metadata writes for commit plans that carry
  new metadata, keeping the Sail-prepared metadata JSON, the written metadata
  object, and the Turso CAS table record in sync.
- Added a LakeCat `metadata-location` commit extension that the Sail-facing
  commit plan validates alongside standard Iceberg REST requirements and threads
  through Turso pointer CAS; aligned the local Grust path dependency with the
  current Grust 0.9 workspace.
- Added metadata pointer compare-and-swap enforcement to Turso commits, including
  expected previous pointer tracking, pointer movement, idempotent replay, and a
  stale-pointer conflict regression test.
- Scaffolded LakeCat as a Rust workspace for an Iceberg-compatible catalog and
  QueryGraph foundation.
- Added REST catalog handlers, typed principal resolution, and integration seams
  for Sail, TypeSec, Grust, OpenLineage, OSI, Croissant, ODRL, and QueryGraph
  bootstrap projection.
- Added Sail-backed scan planning for local Iceberg metadata, including
  structured table-bound plan-task tokens, incremental append planning, delete
  file references, and conservative bounds pruning.
- Added a Rust-native Turso local durable catalog store behind `turso-local`,
  including namespaces, table records, metadata pointer log, idempotency replay,
  audit events, and outbox events.
- Added repo guidance in `AGENTS.md`: push graph behavior into Grust, Iceberg
  and planning work into Sail, governance into TypeSec, and describe each
  logical unit in this changelog before committing.
