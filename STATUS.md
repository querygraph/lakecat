# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `7b2a7cc Resolve Vault credential refs through TypeSec`.
- Cloud CI remains gated on the publish chain: wait for Grust to publish the
  needed crates, then for TypeSec to publish its matching crates, then rebuild
  LakeCat in GitHub Actions against published crates rather than pinning CI to
  unpublished sibling checkout states.
- Automatic GitHub Actions CI is disabled while that publish gate is open. The
  workflow is manual-only via `workflow_dispatch` until the cloud dependency
  graph is locally reproduced and known to work.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In This Commit

- QueryGraph bootstrap bundles now include a `manifest` with schema version,
  producer, standards list, OpenLineage hash, and per-table hashes for the
  Croissant, CDIF, OSI, and ODRL artifacts.
- The manifest is a verification contract for QueryGraph importers. It does not
  move graph taxonomy or traversal behavior into LakeCat.

## Verification Completed

- `cargo test -p lakecat-querygraph`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `cargo fmt --all -- --check` (passes with existing stable-rustfmt warnings for
  nightly-only `imports_granularity` / `group_imports` config keys)
- `git diff --check`

## Next Recommended Slice

Teach QueryGraph's Rust importer to consume and verify the LakeCat bootstrap
manifest, while keeping graph taxonomy and traversal behavior in Grust.
