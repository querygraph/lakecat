# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed LakeCat slice before this continuation:
  `8d02771 Add LakeCat CLI admin setup commands`.
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

- Added `lakecat-cli qglake-fixture`, a live-service setup command that creates a
  demo namespace/table, local storage profile, ODRL policy binding, and verified
  QueryGraph bootstrap bundle through LakeCat APIs.
- This gives LakeCat a repeatable QGLake fixture path against a Turso-backed
  local service without putting graph traversal or taxonomy work in LakeCat.

## Verification Completed

- `cargo fmt --all -- --check` (passes with existing stable-rustfmt warnings for
  nightly-only `imports_granularity` / `group_imports` config keys)
- `cargo check -p lakecat-cli`
- `cargo test -p lakecat-cli`
- Live LakeCat service with `LAKECAT_TURSO_PATH=target/qglake-live/catalog.db`
  and `LAKECAT_BIND_ADDR=127.0.0.1:18281`
- `cargo run -p lakecat-cli -- config --catalog http://127.0.0.1:18281`
- `cargo run -p lakecat-cli -- qglake-fixture --catalog http://127.0.0.1:18281 --output target/qglake-live/lakecat-bootstrap.json`
- `jq` inspection of `target/qglake-live/lakecat-bootstrap.json`
- QueryGraph verifier:
  `cargo run -- lakecat-verify --bundle /Users/alexy/src/lakecat/target/qglake-live/lakecat-bootstrap.json`
  in `/Users/alexy/src/querygraph/qg-rust`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Feed the LakeCat-generated QGLake bootstrap bundle into QueryGraph's verifier
and importer path, then promote any reusable graph-ingest pieces into Grust.
