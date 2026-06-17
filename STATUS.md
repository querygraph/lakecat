# LakeCat Status

Updated: 2026-06-16

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `30f4221 Project nested Iceberg types into Sail status`.
- Current working slice hardens Sail constraint handling in the in-process
  Iceberg provider by rejecting unsupported `UNIQUE` constraints instead of
  silently dropping them from generated metadata.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In This Commit

- `LakeCatCatalogProvider::create_table` validates table constraints before
  building Iceberg metadata.
- Sail `PrimaryKey` constraints continue to map to Iceberg
  `identifier-field-ids`.
- Sail `Unique` constraints now return `CatalogError::InvalidArgument`, avoiding
  silent constraint loss.
- A provider test covers the rejected-unique path and verifies the table is not
  created.

## Verification Completed

- `cargo fmt`
- `cargo test -p lakecat-sail --features catalog-provider catalog_provider::tests -- --nocapture`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Upstream the Iceberg metadata-to-`TableStatus` conversion helpers into Sail once
the sibling Sail WIP is stable, then use those helpers from LakeCat instead of
maintaining a parallel conversion path.
