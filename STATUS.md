# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `554df03 Add LakeCat runtime config controls`.
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

- Added `lakecat-cli` admin commands for local governed demo setup:
  `storage-profile-list`, `storage-profile-upsert`, `policy-list`, and
  `policy-upsert`.
- The commands call LakeCat management endpoints using typed API payloads, so
  Turso-backed QGLake setup can be scripted without bespoke curl snippets.

## Verification Completed

- `cargo fmt --all -- --check` (passes with existing stable-rustfmt warnings for
  nightly-only `imports_granularity` / `group_imports` config keys)
- `cargo check -p lakecat-cli`
- `cargo test -p lakecat-cli`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Use the CLI against a live local LakeCat service with a Turso store to produce a
repeatable QGLake bootstrap fixture, then feed the exported bundle into
QueryGraph's verifier/import path.
