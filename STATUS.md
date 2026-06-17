# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `0792077 Add LakeCat bootstrap export CLI`.
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

- Added predictable local runtime controls for the service binary:
  `LAKECAT_WAREHOUSE` and `LAKECAT_BIND_ADDR`.
- Added `lakecat-cli config`, a smoke-test command that fetches
  `/catalog/v1/config`, validates the Iceberg REST config response, and prints
  it as JSON.

## Verification Completed

- `cargo fmt --all -- --check` (passes with existing stable-rustfmt warnings for
  nightly-only `imports_granularity` / `group_imports` config keys)
- `cargo test -p lakecat-cli`
- `cargo check -p lakecat-service`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Add CLI admin commands for local Turso-backed demos, starting with storage
profile and ODRL policy-binding upsert/list flows, so QGLake acceptance setup can
be scripted without bespoke curl snippets.
