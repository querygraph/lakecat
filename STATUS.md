# LakeCat Status

Updated: 2026-06-16

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `69d5a5a Changelog entries for code-review fixes`.
- Current working slice adds Iceberg identifier-field projection to the
  in-process Sail provider, preserving Sail primary-key constraints as Iceberg
  schema `identifier-field-ids` and projecting loaded identifier fields back to
  Sail `CatalogTableConstraint::PrimaryKey`.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In This Commit

- `LakeCatCatalogProvider::create_table` writes Iceberg
  `identifier-field-ids` from Sail primary-key constraints.
- `table_status` resolves Iceberg identifier field ids through the current schema
  and populates Sail primary-key constraints.
- The in-process provider test round-trips primary-key constraints through
  LakeCat metadata.

## Verification Completed

- `cargo fmt`
- `cargo test -p lakecat-sail --features catalog-provider provider_resolves_governed_tables_in_process -- --nocapture`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Continue the Sail `TableStatus` conversion with nested Iceberg types and any
remaining constraint forms, then upstream reusable Iceberg metadata conversion
helpers into Sail once the sibling Sail WIP is stable.
