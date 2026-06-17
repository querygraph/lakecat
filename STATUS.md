# LakeCat Status

Updated: 2026-06-17

## Current State

- LakeCat is on `master`.
- Latest committed and pushed LakeCat implementation slice:
  `c04f833 Reconcile Grust Cypher and TypeSec 0.8`.
- Status commit recording the pushed Grust Cypher reconciliation:
  `e6ca9e0 Record Grust Cypher reconciliation status`.
- Status commits recording the pushed verifier work:
  `f68cc05 Record TypeDID verifier status` and
  `d720dc4 Record pushed TypeDID verifier status`.
- Supporting TypeSec attestation commit exists locally in `/Users/alexy/src/typesec`
  and is pushed as `0ec164a Add TypeDID verification attestations`.
- Current local dependency reconciliation is complete: LakeCat now targets
  `typesec` 0.8.0 and Grust 0.9.0 with the `cypher` facade feature enabled for
  `grust-local`.
- Supporting Sail helper commit exists locally in `/Users/alexy/src/sail` as
  `68631016 Expose Iceberg table status conversion`. Pushing to
  `lakehq/sail` is blocked for this machine/account: HTTPS has no configured
  credential prompt, and SSH reports permission denied for `alexy`.
- Additional supporting Sail helper commit exists locally as
  `fdb3b657 Expose Iceberg planning result helpers`; it has the same upstream
  push blocker until Sail repository credentials/permissions are resolved.
- Cloud CI remains manual-only. The local dependency chain is being reconciled
  first against Grust 0.9.0 (`grust-cypher` included) and TypeSec 0.8.0; automatic
  GitHub Actions should stay disabled until the same graph is green in the cloud.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In This Commit

- Bumped the LakeCat TypeSec path dependency to 0.8.0.
- Enabled Grust's `cypher` facade feature for `grust-local` graph integration.
- Added a LakeCat graph boundary test that writes the Grust-owned catalog graph
  projection to `MemoryGraphStore` and verifies Grust Cypher can mutate/query it
  without LakeCat owning Cypher parsing, traversal, or graph execution.
- Added a `typesec-local` TypeDID verifier seam in LakeCat service that asks
  TypeSec to open and verify protected TypeDID envelopes.
- Authorization now upgrades anonymous or matching supplied request identity to
  the verified DID subject and attaches only an audit-safe attestation plus
  envelope hash to the authorization context.
- TypeSec now exposes `TypeDidAttestation` from verified TypeDID messages so
  LakeCat can persist receipts without raw plaintext payloads or signatures.
- Exported Sail's Iceberg REST `LoadTableResult` to `TableStatus` conversion as
  a reusable `sail-catalog-iceberg` helper.
- Updated LakeCat's in-process Sail `CatalogProvider` to use the Sail helper for
  stable Iceberg metadata and keep only LakeCat-specific properties plus the v4
  extension fallback local.
- Added Sail-owned Iceberg REST planning-result helpers and updated LakeCat's
  Sail-backed scan planning/fetch path to validate generated standard response
  payloads through them before returning LakeCat extension fields.
- Added a reusable LakeCat catalog-event graph taxonomy helper to Grust, covering
  catalog events, warehouses, namespaces, and tables with stable containment
  edges.
- Updated LakeCat's `grust-local` graph sink to call the Grust helper and pass
  durable outbox event ids into graph event vertices.
- Added a reusable LakeCat catalog graph adapter to Grust in commit
  `15952a9 Add LakeCat catalog graph adapter`.
- Updated QueryGraph in commit `657fd6a Validate LakeCat imports with Grust`
  so `lakecat-import` validates LakeCat graph envelopes with Grust and reports
  graph size from the validated graph.
- Verified the LakeCat-generated QGLake bundle through QueryGraph's
  `lakecat-import` path.
- QueryGraph now checks the outer LakeCat bundle hash, validates the graph
  envelope through Grust, and writes an import plan for downstream graph
  ingestion.
- No graph taxonomy, traversal, or ingest mechanics moved into LakeCat; reusable
  graph work remains targeted at Grust.

## Verification Completed

- `cargo fmt -p lakecat-graph -p lakecat-service -- --check`
- `cargo test -p lakecat-graph --features grust-local grust_cypher_can_query_lakecat_catalog_projection_boundary -- --nocapture`
- `cargo test -p lakecat-graph --features grust-local -- --nocapture`
- `cargo test --workspace`
- `cargo test -p lakecat-service --features typesec-local -- --nocapture`
- `cargo test --workspace --all-features`
- Grust focused check in `/Users/alexy/src/grust`:
  `cargo test -p grust-graph --features cypher,memory lakecat -- --nocapture`
- TypeSec focused check in `/Users/alexy/src/typesec`:
  `cargo test -p typesec-integrations typedid_verified_message_exposes_audit_safe_attestation -- --nocapture`
- `cargo fmt -p lakecat-service -- --check`
- `cargo fmt -p typesec-integrations -p typesec -- --check`
- `cargo test -p typesec-integrations typedid_verified_message_exposes_audit_safe_attestation -- --nocapture`
- `cargo test -p typesec-integrations typedid_signature_covers_conversation_metadata -- --nocapture`
- `cargo test -p lakecat-service --features typesec-local typesec_typedid_envelope_verification_updates_authorization_context -- --nocapture`
- `cargo test -p lakecat-service --features typesec-local -- --nocapture`
- `cargo test -p lakecat-service --all-features`
- `cargo test --workspace --all-features`
- `git diff --check`
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
- QueryGraph importer:
  `cargo run -- lakecat-import --bundle /Users/alexy/src/lakecat/target/qglake-live/lakecat-bootstrap.json --output .querygraph/lakecat/import-plan.json`
  in `/Users/alexy/src/querygraph/qg-rust`
- Grust facade tests: `cargo test -p grust-graph`
- Sail Iceberg catalog test:
  `cargo test -p sail-catalog-iceberg test_get_table -- --nocapture`
- LakeCat Sail catalog-provider tests:
  `cargo test -p lakecat-sail --features catalog-provider catalog_provider -- --nocapture`
- Sail Iceberg planning helper tests:
  `cargo test -p sail-catalog-iceberg planning -- --nocapture`
- LakeCat Sail scan-planning tests:
  `cargo test -p lakecat-sail --features sail-local validates_scan_with_sail_rest_models -- --nocapture`
  and
  `cargo test -p lakecat-sail --features sail-local expands_local_manifest_list_with_sail_io -- --nocapture`
- LakeCat service Sail scan/fetch tests:
  `cargo test -p lakecat-service --features sail-local fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens -- --nocapture`
  and
  `cargo test -p lakecat-service --features sail-local create_load_commit_and_plan_table_round_trips_through_integrations -- --nocapture`
- LakeCat graph tests: `cargo test -p lakecat-graph --features grust-local`
- LakeCat service outbox test:
  `cargo test -p lakecat-service --features grust-local outbox_drain_projects_table_events_to_sinks -- --nocapture`
- Grust Sail compile check: `cargo check -p grust-sail`
- QueryGraph tests: `cargo test` in `/Users/alexy/src/querygraph/qg-rust`
- `cargo test --workspace --all-features`
- `git diff --check`

## Next Recommended Slice

Finish the local dependency reconciliation by running the default and
all-feature LakeCat gates against Grust 0.9.0 plus TypeSec 0.8.0, then update the
cloud workflow only after the same dependency graph is known to build outside the
local sibling checkout.
