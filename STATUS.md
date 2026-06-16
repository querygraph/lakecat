# LakeCat Status

Updated: 2026-06-16

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `617d33c Gate credential issuance through TypeSec`.
- Current working slice adds Iceberg default sort-order projection to the
  in-process Sail provider, preserving `CreateTableOptions.sort_by` as Iceberg
  sort-order metadata and projecting loaded table metadata back to Sail
  `TableStatus.sort_by`.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In This Commit

- `LakeCatCatalogProvider::create_table` now writes Iceberg `sort-orders` from
  Sail `CreateTableOptions.sort_by`.
- `table_status` now resolves the current/default Iceberg sort order through the
  current schema and populates Sail `CatalogTableSort` values.
- The in-process provider test now round-trips ascending and descending sort
  fields through LakeCat metadata.

## Verification Completed

- `cargo fmt`
- `cargo test -p lakecat-sail --features catalog-provider provider_resolves_governed_tables_in_process -- --nocapture`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Continue the Sail `TableStatus` conversion with nested types, identifier fields,
and constraints, then upstream reusable Iceberg metadata conversion helpers into
Sail once the sibling Sail WIP is stable.
