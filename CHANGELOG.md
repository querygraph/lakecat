# Changelog

## Unreleased

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
