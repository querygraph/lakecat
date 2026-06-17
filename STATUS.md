# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `20d87a2 Add QueryGraph bootstrap manifest hashes`.
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

- Added a `lakecat-cli` crate with `bootstrap-export`, an operator command that
  fetches `/querygraph/v1/bootstrap`, verifies the LakeCat manifest hashes, and
  writes the bundle for QueryGraph import.
- Moved reusable QueryGraph bootstrap manifest verification into
  `lakecat-querygraph` so LakeCat tools and importers use one contract.

## Verification Completed

- `cargo fmt --all -- --check` (passes with existing stable-rustfmt warnings for
  nightly-only `imports_granularity` / `group_imports` config keys)
- `cargo check -p lakecat-cli`
- `cargo test -p lakecat-querygraph`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Add a small `lakecat-cli` conformance/admin path for local Turso-backed demos, or
continue tightening runtime config so the service can be started predictably with
explicit warehouse, bind address, and integration choices.
