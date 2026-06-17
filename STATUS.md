# LakeCat Status

Updated: 2026-06-16

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `bee2986 Project Iceberg identifier fields into Sail status`.
- Current working slice adds nested Iceberg type projection to the in-process
  Sail provider, preserving struct/list/map metadata as Arrow/DataFusion nested
  types and allocating Iceberg nested field ids when Sail creates metadata.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In This Commit

- `iceberg_type_to_datafusion` now parses Iceberg `struct`, `list`, and `map`
  type objects instead of falling back to UTF-8.
- `datafusion_type_to_iceberg` now emits nested Iceberg type objects with stable
  nested field ids when creating tables through the Sail provider.
- The provider test now round-trips a nested struct/list column while preserving
  partition, sort, and primary-key projections.
- A focused nested-type test covers Iceberg struct/list/map parsing.

## Verification Completed

- `cargo fmt`
- `cargo test -p lakecat-sail --features catalog-provider catalog_provider::tests -- --nocapture`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Continue the Sail `TableStatus` conversion with remaining constraint forms and
then upstream reusable Iceberg metadata conversion helpers into Sail once the
sibling Sail WIP is stable.
