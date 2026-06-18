# LakeCat Status

Updated: 2026-06-18

## Current State

- LakeCat is on `master`.
- Latest committed and pushed LakeCat implementation slice:
  `d9c8ac7 Require QGLake fetched data files`.
- Paused after pushing the QGLake fetched-data-file proof slice. The
  `lakecat-cli qglake-fixture` governed `fetchScanTasks` verifier now rejects
  placeholder scan-task JSON and requires at least one fetched file scan task to
  carry an Iceberg REST `data-file.file-path`, proving acceptance sees actual
  data-file work from Sail manifest expansion.
- Local verification for the pushed QGLake fetched-data-file proof slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`.
- This status commit records the pushed QGLake fetched-data-file proof slice.
- Previous implementation slice:
  `e02045b Require QGLake fetched scan work`.
- Paused after pushing the QGLake fetched-scan-work proof slice. The
  `lakecat-cli qglake-fixture` governed `fetchScanTasks` verifier now rejects
  responses that carry only the residual policy proof but no fetched file scan
  tasks, proving the plan-task token was expanded into real Sail-backed scan
  work.
- Local verification for the pushed QGLake fetched-scan-work proof slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fetch_scan_tasks_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`.
- This status commit records the pushed QGLake fetched-scan-work proof slice.
- Previous implementation slice:
  `d5b496a Preflight QGLake manifest lists`.
- Paused after pushing the QGLake manifest-list preflight slice. QGLake fixture
  reruns now reject existing fixture tables when snapshot manifest-list files
  referenced by the table metadata are missing locally, failing before live
  governed plan/fetch verification reaches Sail's manifest expansion.
- Local verification for the pushed QGLake manifest-list preflight slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_existing_table_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`.
- This status commit records the pushed QGLake manifest-list preflight slice.
- Previous implementation slice:
  `d38d1f2 Validate QGLake metadata pointer reruns`.
- Paused after pushing the QGLake metadata-pointer rerun validation slice.
  `lakecat-cli qglake-fixture` now rejects existing fixture tables when the
  advertised local `metadata_location` file is missing, invalid, or drifted
  from the Iceberg metadata returned by the catalog, so reruns cannot silently
  accept a non-openable metadata pointer.
- Local verification for the pushed QGLake metadata-pointer rerun validation
  slice was green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_existing_table_verifier`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`.
- This status commit records the pushed QGLake metadata-pointer rerun
  validation slice.
- Previous implementation slice:
  `e201570 Write QGLake metadata pointer file`.
- Paused after pushing the QGLake metadata-pointer file slice. The local
  QGLake fixture now writes the Iceberg table metadata JSON at the advertised
  `metadata_location` in addition to the manifest list and data manifest, so
  standard metadata-pointer consumers can open the bootstrap table metadata
  file directly.
- Local verification for the pushed QGLake metadata-pointer file slice was
  green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fixture_metadata_contains_restricted_raw_payload_column`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`.
- This status commit records the pushed QGLake metadata-pointer file slice.
- Previous implementation slice:
  `97b6e60 Make QGLake fixture fetchable`.
- Paused after pushing the QGLake fetchable fixture slice. `lakecat-cli
  qglake-fixture` now creates local Iceberg manifest-list and data-manifest
  files with Sail's Iceberg writer types, records a current snapshot in the
  bootstrap table metadata, rejects existing QGLake tables that cannot support
  governed scan planning and `fetchScanTasks`, and keeps OPUS2-DESIGN aligned
  with the now-built ODRL restriction/plan/fetch proof.
- Local verification for the pushed QGLake fetchable fixture slice was green:
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`;
  `cargo test -p lakecat-cli`;
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake_fixture_metadata_contains_restricted_raw_payload_column`;
  `git diff --check`.
- This status commit records the pushed QGLake fetchable fixture slice.
- Previous implementation slice:
  `d0dc194 Stamp governed receipts with policy hashes`.
- Paused after pushing governed receipt policy-hash stamping. Governed scan and
  credential-vend authorization receipts now get a deterministic top-level
  `policy_hash` derived from enforced `ReadRestriction` policy hashes while
  preserving any governance-engine hash as an input. The service authorization
  boundary and in-process Sail provider scan path both use the shared
  `lakecat-security` receipt helper.
- Local verification for the pushed governed receipt policy-hash slice was
  green:
  `cargo fmt -p lakecat-security -p lakecat-service -p lakecat-sail -- --check`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-sail --features catalog-provider`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`.
- This status commit records the pushed governed receipt policy-hash slice.
- Previous implementation slice:
  `5a20750 Expose governed fetch restriction proof`.
- Paused after pushing the governed fetch restriction proof. Iceberg REST
  `fetchScanTasks` responses now surface a `lakecat:fetch-scan-tasks`
  extension carrying the re-applied `ReadRestriction`, and the QGLake verifier
  now requires a governed plan-task token whose fetch response carries the same
  policy hash proof as the scan plan.
- Local verification for the pushed governed fetch restriction proof slice was
  green:
  `cargo fmt -p lakecat-service -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-service fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens --features sail-local,turso-local`;
  `cargo test -p lakecat-service --features sail-local,turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `git diff --check`.
- This status commit records the pushed governed fetch restriction proof slice.
- Previous implementation slice:
  `b4b5116 Add Sail provider namespace drop`.
- Paused after pushing the Sail provider namespace-drop bridge. The in-process
  Sail `CatalogProvider` now routes `drop_database` through LakeCat's governed
  durable namespace deletion with typed `namespace.drop` capability validation,
  `if_exists` handling, and explicit rejection of unsupported cascading drops.
  LakeCat's store remains the enforcement point for non-empty namespace guards.
- Local verification for the pushed Sail provider namespace-drop slice was
  green:
  `cargo fmt -p lakecat-sail -- --check`;
  `cargo test -p lakecat-sail --features catalog-provider provider_drops_durable_namespaces`;
  `cargo test -p lakecat-sail --features catalog-provider`;
  `git diff --check`.
- This status commit records the pushed Sail provider namespace-drop slice.
- Previous implementation slice:
  `28c044e Require QGLake scan policy hash proof`.
- Paused after pushing the QGLake scan policy-hash proof. The QGLake governed
  scan verifier now requires the enforced `ReadRestriction` to carry the
  expected hash of the bootstrapped ODRL policy document, proving the accepted
  scan is bound to the policy that defined the allowed columns and row
  predicate rather than merely carrying a lookalike restriction.
- Local verification for the pushed QGLake scan policy-hash proof was green:
  `cargo fmt -p lakecat-cli -- --check`;
  `cargo test -p lakecat-cli qglake`;
  `git diff --check`;
  `cargo test -p lakecat-service scan_planning_applies_policy_column_restriction_before_sail --features sail-local,turso-local`.
- This status commit records the pushed QGLake scan policy-hash proof slice.
- Previous implementation slice:
  `cbda084 Add Sail provider view column bridge`.
- Paused after pushing the Sail provider view-column bridge. `ViewRecord` now
  persists typed view columns in memory and Turso, the management/catalog view
  APIs accept and return those columns, QueryGraph bootstrap exports them in
  view projections and OSI handoff, and the in-process Sail `CatalogProvider`
  can create, load, list, and drop durable LakeCat views as `TableKind::View`
  statuses. Full Iceberg view version/commit semantics remain pending.
- Local verification for the pushed Sail provider view-column bridge slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-store -p lakecat-service -p lakecat-querygraph -p lakecat-sail -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-sail --features catalog-provider provider_manages_durable_views_with_typed_columns -- --nocapture`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-querygraph projects_catalog_views_into_querygraph_bundle`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_view_records`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-querygraph`;
  `cargo test -p lakecat-sail --features catalog-provider`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed Sail provider view-column bridge slice.
- Previous implementation slice:
  `b73f503 Add governed namespace load and drop`.
- Paused after pushing governed namespace load/drop. Unprefixed and
  warehouse-prefixed Iceberg REST catalog paths can now load and delete durable
  namespace state through typed `namespace.load` and `namespace.drop`
  capabilities. Namespace drops are blocked while tables, views, or scoped
  policy bindings remain, and audited `namespace.dropped` events replay into
  graph/lineage projection as deleted namespace events.
- Local verification for the pushed governed namespace load/drop slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -p lakecat-lineage -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-security table_capabilities_require_matching_allowed_receipts`;
  `cargo test -p lakecat-store memory_store_loads_and_drops_namespaces`;
  `cargo test -p lakecat-store --features turso-local turso_store_loads_and_drops_namespaces`;
  `cargo test -p lakecat-service namespaces_load_and_drop_through_catalog_routes -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_table_events_to_sinks -- --nocapture`;
  `cargo test -p lakecat-lineage`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed governed namespace load/drop slice.
- Previous implementation slice:
  `5d53aea Add governed durable view drop`.
- Paused after pushing governed durable view deletion. Management and
  warehouse-prefixed catalog REST paths can now delete durable `ViewRecord`
  values from memory and Turso stores through a typed `view.drop` capability,
  emitting audited `view.dropped` events while preserving Iceberg table access
  semantics.
- Local verification for the pushed governed view drop slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service`;
  `cargo test -p lakecat-security table_capabilities_require_matching_allowed_receipts`;
  `cargo test -p lakecat-store memory_tests::memory_store_persists_view_records`;
  `cargo test -p lakecat-store --features turso-local turso_store::tests::turso_store_persists_view_records`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed governed view drop slice.
- Previous implementation slice:
  `d31ace5 Exercise production secret-ref gating`.
- Paused after pushing production secret-ref gate coverage. The TypeSec-backed
  credential issuer now has service-level coverage proving `vault://`,
  `aws-sm://`, `gcp-sm://`, and `azure-kv://` refs authorize the exact secret
  URI before failing closed with provider-specific not-configured errors when no
  resolver backend is wired.
- Local verification for the pushed production secret-ref gating slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo test -p lakecat-service --features typesec-local typesec_credential_issuer_gates_production_secret_refs_before_dispatch -- --nocapture`;
  `cargo fmt -p lakecat-service -p lakecat-api -p lakecat-security -p lakecat-store -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-service --features typesec-local`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed production secret-ref gating slice.
- Previous implementation slice:
  `efd548a Add catalog view REST aliases`.
- Paused after pushing catalog-path view REST aliases. Warehouse-prefixed catalog
  callers can now list, load, and upsert durable `ViewRecord` values through
  `/catalog/v1/{warehouse}/namespaces/{namespace}/views`, with governed
  `view.load` authorization for reads and audited Iceberg REST `view.*` events.
- Local verification for the pushed catalog view REST alias slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-security table_capabilities_require_matching_allowed_receipts`;
  `cargo test -p lakecat-store memory_tests::memory_store_persists_view_records`;
  `cargo test -p lakecat-store --features turso-local turso_store::tests::turso_store_persists_view_records`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed catalog view REST alias slice.
- Previous implementation slice:
  `2680f95 Add project-scoped warehouse management`.
- Paused after pushing project-scoped warehouse management routes. Management
  callers can now list and upsert warehouses through
  `/management/v1/projects/{project}/warehouses`, using the same durable
  `WarehouseRecord` state and validation while leaving standard Iceberg table
  routes unchanged.
- Local verification for the pushed project-scoped warehouse management slice was
  green:
  `cargo fmt -p lakecat-api -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_warehouse_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_warehouse_records`;
  `cargo test -p lakecat-service management_warehouses_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed project-scoped warehouse management
  slice.
- Previous implementation slice:
  `e200664 Validate warehouse project attachments`.
- Paused after pushing warehouse-to-project validation. Durable warehouse upserts
  in memory and Turso now reject references to missing projects, and the service
  management/prefixed-route tests create the durable project before registering
  warehouses.
- Local verification for the pushed warehouse-project validation slice was green:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_warehouse_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_warehouse_records`;
  `cargo test -p lakecat-service management_warehouses_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed warehouse-project validation slice.
- Previous implementation slice:
  `9a27369 Attach projects to durable servers`.
- Paused after pushing optional `server-id` attachment for durable project
  records. Project management responses now expose the Server > Project link,
  and memory/Turso stores reject project attachments to missing servers while
  preserving existing projects that do not declare a server.
- Local verification for the pushed project-server attachment slice was green:
  `cargo fmt -p lakecat-api -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_project_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_project_records`;
  `cargo test -p lakecat-service management_projects_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed project-server attachment slice.
- Previous implementation slice:
  `7bc33ab Add governed durable server records`.
- Paused after pushing governed durable `ServerRecord` support with management
  list/upsert endpoints, memory/Turso persistence, `server.manage`
  authorization, audited `server.*` events, and docs updated to reflect the
  Server > Project > Warehouse control-plane hierarchy now starting in code.
- Local verification for the pushed governed-server slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_server_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_server_records`;
  `cargo test -p lakecat-service management_servers_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- This status commit records the pushed governed-server slice.
- Previous implementation slice:
  `a6669e5 Export views in QueryGraph bootstrap`.
- Local verification for the previous QueryGraph view-bootstrap slice was green:
  `cargo fmt -p lakecat-cli -p lakecat-querygraph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-querygraph projects_catalog_views_into_querygraph_bundle -- --nocapture`;
  `cargo test -p lakecat-service querygraph_bootstrap_projects_catalog_views -- --nocapture`;
  `cargo test -p lakecat-cli qglake_bootstrap -- --nocapture`;
  `cargo test -p lakecat-querygraph`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Previous implementation slice:
  `1c3cfb8 Add governed durable view records`.
- Local verification for the previous governed-view slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_view_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_view_records`;
  `cargo test -p lakecat-service management_views_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Prior implementation slice:
  `d905f27 Route commit metadata through object_store`.
- Local verification for the previous object-store metadata writer slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo fmt -p lakecat-service -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-service commit_can_advance_metadata_location_extension -- --nocapture`;
  `cargo test -p lakecat-service --all-features stale_commit_cleans_up_uncommitted_metadata_file -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Prior implementation slice:
  `ce4b82b Verify QGLake credential blocking`.
- Local verification for the previous QGLake credential-blocking slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `cargo fmt -p lakecat-cli -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-cli qglake`;
  `cargo test -p lakecat-cli`;
  `cargo test -p lakecat-service credential_vend_blocks_raw_credentials_for_fine_grained_restriction -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Prior implementation slice:
  `109e0dd Block credential bypass for restricted reads`.
- Local verification for the previous credential-bypass hardening slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-service credential_vend_blocks_raw_credentials_for_fine_grained_restriction -- --nocapture`;
  `cargo test -p lakecat-service credentials_vend_audit_payload_surfaces_policy_context`;
  `cargo test -p lakecat-security read_restriction`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Local verification for the previous registered-prefix slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_warehouse_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_warehouse_records`;
  `cargo test -p lakecat-service prefixed_catalog_routes_target_requested_warehouse -- --nocapture`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- The all-feature gates again required local syntax repairs in the dirty sibling
  `/Users/alexy/src/grust/crates/grust-cypher/src/lib.rs` checkout around
  return-projection helper edits. LakeCat did not stage the sibling Grust repo.
- Local verification for the previous management-routing slice was green:
  `cargo fmt -p lakecat-service`;
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-service management_warehouses_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- Local verification for the previous project-management slice was green:
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service`;
  `cargo fmt -p lakecat-api -p lakecat-security -p lakecat-store -p lakecat-graph -p lakecat-service -- --check`;
  `git diff --check`;
  `cargo test -p lakecat-store memory_store_persists_project_records`;
  `cargo test -p lakecat-store --features turso-local turso_store_persists_project_records`;
  `cargo test -p lakecat-service management_projects_are_durable_management_entities -- --nocapture`;
  `cargo test -p lakecat-service outbox_drain_projects_project_upserts_to_graph -- --nocapture`;
  `cargo test -p lakecat-graph --features grust-local converts_project_event_to_valid_grust_graph_event`;
  `cargo test -p lakecat-store`;
  `cargo test -p lakecat-security`;
  `cargo test -p lakecat-graph`;
  `cargo test -p lakecat-graph --features grust-local`;
  `cargo test -p lakecat-store --features turso-local`;
  `cargo test -p lakecat-service`;
  `cargo test -p lakecat-service --features turso-local`;
  `cargo test -p lakecat-service --all-features`;
  `cargo test --workspace`;
  `cargo test --workspace --all-features`.
- The `grust-local` gates required local syntax repairs in the dirty sibling
  `/Users/alexy/src/grust/crates/grust-cypher/src/lib.rs` checkout around the
  current return-projection helper edits. LakeCat did not stage the sibling Grust
  repo.
- The all-feature service/workspace gates required another one-line local Grust
  fix in `/Users/alexy/src/grust/crates/grust-cypher/src/lib.rs`: a stale
  recursive `evaluate_scalar_return_projection` call still passed
  `&split.target` after the current dirty Grust checkout changed the callee
  signature. LakeCat did not stage the dirty sibling Grust repo.
- The all-feature service/workspace gates required a one-token local Grust fix
  in `/Users/alexy/src/grust/crates/grust-cypher/src/lib.rs`: an accidental
  duplicated `target` argument inside a `matches!` macro in the current dirty
  Grust checkout. The Grust repo had broad pre-existing uncommitted edits, so
  LakeCat did not stage that sibling repo.
- Manual cloud gate status: run `27722995692` was started only after local
  workflow reproduction. It completed with all focused rows green, including
  default workspace, `sail-local service`, `typesec-local service`,
  `typesec-local security`, `grust-local graph`, `grust cypher boundary`, and
  `turso-local store`. The only failing row was `all features workspace`, where
  `lakecat-sail::sail_integration::tests::preserves_filter_context_and_prunes_loaded_file_bounds`
  panicked on an unwrap in the cloud checkout at `22827ee`; the same focused
  command passes locally on the current worktree. Automatic push/PR CI remains
  disabled.
- LakeCat carries a temporary CI bridge under
  `ci/sail-patches/` that applies local Sail helper commits `68631016` and
  `fdb3b657`, plus the generated-model module export LakeCat service tests use,
  to the `lakehq/sail@main` checkout before building. The bridge now supplies
  a local `git am` committer identity and passes absolute patch paths so
  `git -C sail` can apply them; it should be removed once those APIs are
  available from an upstream Sail branch.
- Status commit recording the pushed Grust Cypher reconciliation:
  `e6ca9e0 Record Grust Cypher reconciliation status`.
- Status commits recording the pushed verifier work:
  `f68cc05 Record TypeDID verifier status` and
  `d720dc4 Record pushed TypeDID verifier status`.
- Supporting TypeSec commits are pushed through
  `e05460f Prepare Typesec 0.8.0 release`.
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
- Cloud CI remains manual-only. The local dependency chain is green against
  Grust 0.9.0 (`grust-cypher` included) and TypeSec 0.8.0; automatic GitHub
  Actions should stay disabled until the same graph is green in the cloud.
- Graph-related implementation is still intentionally kept out of LakeCat unless
  it is a bounded outbox/projection concern. Reusable graph taxonomy and graph
  mechanics belong in `/Users/alexy/src/grust`.
- Sail remains the target for planner/table-status work, but `/Users/alexy/src/sail`
  has separate graph-extension WIP and should not be edited casually.

## Completed In Latest Implementation Slice

- Added project-scoped warehouse management routes for listing and upserting
  warehouses under durable projects without changing standard Iceberg table
  routes.
- Enforced warehouse-to-project attachment in memory and Turso stores, with
  governed warehouse management rejecting warehouses that point at missing
  projects.
- Added optional `server-id` attachment to durable project records, surfaced it
  through management API responses, and made memory/Turso project upserts reject
  attachments to missing servers.
- Added governed durable server records with management list/upsert endpoints,
  memory/Turso persistence, and audited `server.*` events as the root of the
  Server > Project > Warehouse control-plane hierarchy.
- Added stored view projections to QueryGraph bootstrap bundles, including
  manifest view artifact hashes, view-aware graph edges, OpenLineage view counts,
  service-level export, and verification coverage.
- Added governed durable view records with management list/upsert endpoints,
  memory/Turso persistence, and audited `view.*` events as the next
  Lakekeeper-style catalog entity after Project and Warehouse.
- Routed commit metadata object writes and stale-write cleanup through
  `object_store::parse_url_opts`, preserving local `file://` behavior while
  opening the writer boundary for configured object-store backends.
- Extended `lakecat-cli qglake-fixture` to prove the restricted QGLake table does
  not receive raw credentials, keeping the acceptance path on governed
  Sail-planned reads.
- Blocked raw credential vending for authorization receipts that carry
  fine-grained row or column read restrictions, and audited the blocked attempt
  with the same policy context.
- Required warehouse-prefixed Iceberg REST catalog routes to resolve a stored
  warehouse record before catalog state changes or loads.
- Added warehouse-prefixed Iceberg REST catalog routes while preserving
  unprefixed default-warehouse routes for existing clients.
- Allowed governed warehouse management endpoints to manage a second durable
  warehouse from the same service instead of rejecting non-default warehouse
  path parameters.
- Added governed project list/upsert management endpoints and a
  `ProjectManage` capability action.
- Added durable `ProjectRecord` persistence to memory and Turso stores.
- Added catalog-facing Project graph events and durable `project.upserted`
  outbox replay to the graph sink.
- Added governed warehouse list/upsert management endpoints and a
  `WarehouseManage` capability action.
- Added durable `WarehouseRecord` persistence to memory and Turso stores.
- Added catalog-facing Warehouse graph events and durable `warehouse.upserted`
  outbox replay to the graph sink.
- Added stable catalog-facing `Column` and `Snapshot` graph event constructors
  plus default and `grust-local` graph-crate coverage.
- Added compact table metadata graph summaries to table create/load audit
  payloads and replayed those summaries from the durable outbox into column and
  snapshot graph anchors.
- Added principal graph projection to durable outbox drain for non-anonymous
  resolved principals, while preserving existing domain graph and OpenLineage
  replay.
- Added a stable principal subject helper plus default and `grust-local`
  graph-crate coverage for the catalog-facing principal event shape.
- Added commit graph projection to durable outbox drain for `table.commit`, while
  preserving the existing table graph and OpenLineage replay.
- Added a stable commit subject helper plus default and `grust-local`
  graph-crate coverage for the catalog-facing commit event shape.
- Added scan-plan graph projection to durable outbox drain for
  `table.scan-planned` and `table.scan-tasks-fetched`, while preserving the
  existing table graph and OpenLineage replay.
- Added a stable scan-plan subject helper plus default and `grust-local`
  graph-crate coverage for the catalog-facing scan-plan event shape.
- Added policy-binding graph projection to the durable outbox drain, so
  `policy-binding.upserted` events now replay to the graph sink with ODRL and
  authorization payloads intact.
- Added a stable policy subject helper plus default and `grust-local` graph-crate
  coverage for the catalog-facing policy event shape.
- Added namespace graph projection to the durable outbox drain, so
  `namespace.created` events now replay to both graph and lineage sinks.
- Added a stable namespace subject helper and graph-crate unit coverage for the
  catalog-facing namespace event shape.
- Added verified QueryGraph bootstrap bundle, graph, OpenLineage, standards, and
  table hash evidence to the `querygraph.bootstrap` audit/outbox payload.
- Extended lineage replay tests to prove bootstrap hash evidence survives the
  outbox drain into OpenLineage-shaped events.
- Added a QueryGraph bootstrap manifest `graph-hash` and verification failure
  for graph projection drift.
- Made `lakecat-cli qglake-fixture` verify the QGLake table graph node and
  namespace edge before accepting the bootstrap bundle.
- Extended the lineage drain API response with delivered event types, graph
  event count, and lineage event count.
- Made `lakecat-cli qglake-fixture` require the drain summary to include
  `querygraph.bootstrap` and at least one lineage emission.
- Added embedded memory-store audit/outbox delivery parity for explicit catalog
  audit events, matching the Turso lineage-and-graph outbox envelope.
- Made `lakecat-cli qglake-fixture` reject a lineage drain that delivers zero
  events, turning the QGLake acceptance run into a real replay check.
- Added a governed lineage-drain endpoint and CLI command, and wired
  `lakecat-cli qglake-fixture` to drain lineage/outbox events after writing the
  verified QueryGraph bootstrap bundle.
- Projected `querygraph.bootstrap` outbox events into LakeCat OpenLineage output
  events while preserving bootstrap authorization/request-identity payloads.
- Added QGLake-specific QueryGraph bootstrap verification to
  `lakecat-cli qglake-fixture`, covering policy bindings, ODRL restriction
  export, and OpenLineage output presence before the bootstrap file is written.
- Made `lakecat-cli qglake-fixture` repeatable by accepting existing namespace
  and table resources only after loading and validating that they match the
  expected QGLake fixture shape.
- Added a live governed scan-plan verification step to
  `lakecat-cli qglake-fixture` after policy installation and before bootstrap
  export.
- Added a fixture verifier test proving `raw_payload` is removed from the
  effective projection and the row predicate survives in the scan extension.
- Added `QueryGraphBootstrap::from_tables_with_policy_bindings` and a
  per-table `policy-bindings` projection with its own manifest hash.
- Changed `/querygraph/v1/bootstrap` to load stored table policy bindings and
  include the actual ODRL documents in the QueryGraph table projection and ODRL
  artifact.
- Added a `raw_payload` column to the QGLake fixture table metadata and kept it
  outside the fixture policy's allowed agent columns.
- Extended the QGLake fixture policy with `lakecat:read-restriction` allowed
  columns, row predicate, and max credential TTL, verified through
  `ReadRestriction::from_odrl_policies`.
- Surfaced governed scan-task fetch `read-restriction`, storage location, and
  metadata location in the top-level `table.scan-tasks-fetched` audit/outbox
  payload.
- Routed `table.scan-tasks-fetched` outbox records through the existing scan
  graph/OpenLineage projection path so fetched concrete file work carries the
  governed restriction context to QueryGraph consumers.
- Surfaced governed scan-planning `read-restriction`, storage location, and
  metadata location in the top-level `table.scan-planned` audit/outbox payload,
  matching the nested authorization receipt context.
- Extended the OpenLineage table-scan projection test to prove the governed
  restriction is preserved in the LakeCat catalog dataset facet for QueryGraph
  consumers.
- Extended authorization restriction derivation from scan planning to credential
  vending so raw credential issuance sees the same policy-derived allowed
  columns, row predicate, TTL, purpose, and policy hashes.
- Surfaced governed credential-vending `read-restriction` and
  `lakecat:raw-credential-exception` markers in the top-level
  `credentials.vend-attempted` audit/outbox payload, matching the nested
  authorization receipt context.
- Marked governed credential-vending authorization context with
  `lakecat:raw-credential-exception = true` so audit/outbox receipts distinguish
  the deliberate exception path from the preferred governed Sail-planned reads.
- Added a service test proving the credential issuer receives the governed
  credential authorization receipt with the composed read restriction and raw
  credential exception marker.
- Enabled the TypeSec RBAC feature for LakeCat's `typesec-local` integration.
- Added `TypeSecGovernanceEngine::rbac_from_yaml`, a narrow constructor that
  loads RBAC policy text through TypeSec's `RbacEngine` and returns LakeCat
  errors on invalid policy documents.
- Added `LAKECAT_TYPESEC_RBAC_POLICY` support to the service binary so local
  deployments can boot with a real RBAC fallback policy instead of the
  allow-all placeholder.
- Added focused tests proving RBAC policy loading authorizes matching table
  scan requests and missing/invalid policy files fail closed.
- Extended `ReadRestriction::from_odrl_policies` to parse max credential TTLs
  from top-level, nested `lakecat:read-restriction` / `readRestriction`, and
  ODRL constraint forms.
- Changed TTL composition to keep the shortest governed credential lifetime
  across multiple active policy bindings.
- Added security tests proving constraint-based TTL extraction, shortest-TTL
  composition, and fail-closed rejection for non-numeric TTL constraints.
- Added `LakeCatCatalogProvider::fetch_table_scan_tasks` and
  `fetch_table_scan_tasks_for_ident`, applying the provider-owned scan
  authorization and shared `ReadRestriction` mandatory projection/filter
  requirements before delegating plan-task expansion to Sail.
- Routed REST `sail-local` `fetch-scan-tasks` through the provider fetch seam
  while preserving the direct `FetchScanTasksRequest` path for default builds.
- Added a provider recording-engine test proving policy-derived projection and
  row predicates are passed into Sail during provider-routed fetch expansion.
- Routed REST `sail-local` scan planning through
  `LakeCatCatalogProvider::plan_table_scan_for_ident`, so REST planning now
  uses the provider-owned scan authorization and governed plan seam.
- Enabled `lakecat-sail/catalog-provider` from the service `sail-local` feature
  and preserved the direct `ScanPlanningRequest` path for builds without local
  Sail provider integration.
- Preserved HTTP error semantics across the provider route by mapping provider
  invalid-argument, not-found, conflict, and not-supported failures back to
  LakeCat HTTP errors instead of flattening them to 500s.
- Added `ReadRestriction` to `lakecat-security` and exposed it through
  `TableScanCapability` so the scan capability carries the server-owned
  restriction from the authorization receipt.
- Added governed row-predicate extraction from enforced ODRL policy bindings,
  including nested `lakecat:read-restriction` fields and ODRL row-predicate
  constraints.
- Composed multiple enforced row predicates with `and` so additional bindings
  can only narrow the governed read surface.
- Proved Sail receives the policy-derived row predicate as an accepted scan
  filter while column projection is also narrowed by policy.
- Bound structured Sail plan-task tokens to the effective projection, preserving
  the column narrowing context through manifest-list expansion.
- Reapplied governed projection/filter requirements on `fetch-scan-tasks` by
  recomputing the current `ReadRestriction` in LakeCat and passing mandatory
  requirements into the Sail expansion path.
- Added a negative guard that legacy/no-projection plan-task tokens cannot
  satisfy a governed projection, preventing empty projection from widening to
  all columns during fetch.
- Added `TypeSecGovernanceEngine::with_fallback`, backed by TypeSec's
  `ComposedEngine` with `PriorityOrder`, so LakeCat can compose ODRL-style
  delegation onto an RBAC-style fallback without implementing local policy
  semantics.
- Added a `typesec-local` test proving a delegated primary policy allows
  authorization through a fallback engine while preserving the authorization
  context and policy hash.
- Wired REST table commits to the store's existing idempotency replay by
  parsing `x-lakecat-idempotency-key` and passing it into `TableCommit`.
- Added conservative idempotency-key validation: non-empty ASCII keys up to 128
  characters using alphanumeric characters plus `-`, `_`, `.`, and `:`.
- Added a service-level test proving two REST commits with the same
  idempotency key replay to one table version and one commit-log row.
- Added a `PlannedMetadataWrite` handle for local `file://` metadata writes so
  the service can clean up a newly written metadata JSON object if the store
  commit/CAS step rejects the commit.
- Added a `sail-local` service test proving a stale commit requirement that
  writes a new metadata location returns `409 Conflict` and removes the
  uncommitted metadata file.
- Added `TableCommitRecord::idempotency_key_sha256`, populated from accepted
  keyed commits and serialized through the metadata pointer log, audit payload,
  and outbox payload.
- Extended store and REST commit idempotency tests to prove the durable hash is
  present and the raw idempotency key is not written to the outbox payload.
- Hardened memory and Turso idempotency replay to persist/check a normalized
  idempotency request hash and reject a reused idempotency key when the request
  body differs from the accepted commit.
- Extended REST and Turso idempotency tests to prove mismatched keyed retries
  return conflict and do not create a second table version or pointer-log row.
- Moved ODRL read-restriction parsing and composition into
  `ReadRestriction::from_odrl_policies` in `lakecat-security`.
- Kept the REST authorization path behavior-equivalent by deriving
  table-scan restrictions from stored `PolicyBinding` ODRL documents through
  the shared security primitive.
- Added security-crate tests proving ODRL policy documents compose allowed
  columns by intersection, row predicates by `and`, and purpose/credential TTL
  into one `ReadRestriction`, plus a negative guard for non-object row
  predicates.
- Moved effective projection narrowing, stats-field narrowing, and mandatory
  row-filter extraction from `lakecat-service` into reusable `ReadRestriction`
  methods.
- Updated REST scan planning and fetch-scan-tasks to call the shared
  `ReadRestriction` methods while preserving the existing governed Sail request
  behavior.
- Added the LakeSail book source, publishing runbook, build/validation scripts,
  and stable generated PDF/EPUB/MOBI deliverables under `docs/book/`.
- Kept the generated versioned Kindle EPUB symlink ignored by `.gitignore` while
  validating that it points to the stable `lakesail.epub` deliverable.
- Added `LakeCatCatalogProvider::authorize_table_scan`, which reads table policy
  bindings from the catalog store, composes a shared `ReadRestriction`, and
  returns a `TableScanCapability` carrying the restriction in the authorization
  receipt context.
- Added a provider test proving stored ODRL policy bindings are visible through
  the provider scan capability and preserve the Sail-provider context.
- Added `LakeCatCatalogProvider::plan_table_scan` and a small provider scan
  request type so provider-routed scan planning can apply governed projection
  and row filters before invoking the configured Sail engine.
- Added a recording-engine provider test proving policy-derived projection and
  row predicates are passed into Sail from the provider scan-planning seam.
- Parsed a minimal enforceable ODRL subset from active policy bindings:
  `allowed-columns` / `allowedColumns` at the policy root, in
  `lakecat:read-restriction`, or in ODRL constraints, plus purpose and policy
  hashes.
- Applied allowed-column restrictions before Sail scan planning. Empty client
  projection under a restriction now means the allowed columns, and client
  projection can only narrow within those columns.
- Added tests proving scan authorization carries the restriction and that a
  `sail-local` scan requesting `event_id,payload` is narrowed to `event_id`
  before Sail receives it.
- Added the OPUS2 review/design docs to the tracked design record and updated
  the OPUS2 plan with this first restriction slice.
- Changed the QueryGraph bootstrap OSI artifact from a LakeCat-authored semantic
  model into a QueryGraph-owned OSI handoff. The handoff keeps the manifest hash
  contract and stable field anchors, but no longer publishes LakeCat-owned OSI
  metrics, dimensions, joins, ontology claims, business semantic names, or SQL
  field expressions.
- Updated architecture and OPUS working-plan docs so LakeCat is the catalog
  discovery substrate for OSI import while QueryGraph owns rich semantic model
  authorship.
- Fixed the Sail patch bridge to pass `git am` committer identity in GitHub
  Actions after run `27722483267` failed before tests with "Committer identity
  unknown".
- Fixed the Sail patch bridge path after run `27722686028` failed before tests
  because `../lakecat/ci/sail-patches/*.patch` did not exist from the Actions
  workspace root.
- Fixed the Sail patch bridge again after run `27722752741` showed that
  `git -C sail` resolves expanded relative patch paths from inside the Sail
  checkout; the workflow now computes an absolute `PATCH_DIR`.
- Added `ci/sail-patches/` with the Sail helper/model API patches LakeCat
  already depends on locally.
- Updated manual GitHub Actions to apply those patches to the Sail checkout
  before building LakeCat.
- Recorded that manual run `27720653125` moved past formatting, TypeSec 0.8
  resolution, and `protoc`, and is now blocked on unpublished Sail helper
  commits.
- Added `protobuf-compiler` installation to the manual GitHub Actions workflow
  so Sail's `prost-build` custom build can find `protoc` on Ubuntu runners.
- Scoped the manual GitHub Actions formatting check to the LakeCat workspace
  packages instead of `cargo fmt --all`, so sibling checkout formatting does not
  fail LakeCat's cloud gate before the matrix tests start.
- Expanded manual GitHub Actions coverage to include
  `cargo test -p lakecat-service --features typesec-local`.
- Added an explicit manual CI row for the Grust Cypher catalog-graph boundary
  test.
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

- LakeSail book artifact checks passed:
  `bash docs/book/check_epub_metadata.sh docs/book/dist/lakesail.epub 'lakesail (0.1.0)'`,
  `pdftotext -f 1 -l 1 docs/book/dist/lakesail.pdf -`,
  `pdftotext -f 2 -l 2 docs/book/dist/lakesail.pdf -`, and
  `readlink 'docs/book/dist/lakesail (0.1.0).epub'`.
- Provider-side governed scan authorization checks passed:
  `cargo fmt -p lakecat-sail -- --check`,
  `cargo test -p lakecat-sail --features catalog-provider provider_scan_authorization_carries_policy_restriction -- --nocapture`, and
  `cargo test -p lakecat-sail --features catalog-provider provider_resolves_governed_tables_in_process -- --nocapture`,
  `cargo test -p lakecat-sail --features catalog-provider`,
  `cargo test -p lakecat-sail --all-features`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Provider-side governed scan planning checks passed:
  `cargo fmt -p lakecat-sail -- --check`,
  `cargo test -p lakecat-sail --features catalog-provider provider_scan_planning_applies_policy_restriction_before_sail -- --nocapture`, and
  `cargo test -p lakecat-sail --features catalog-provider provider_scan_authorization_carries_policy_restriction -- --nocapture`,
  `cargo test -p lakecat-sail --features catalog-provider`,
  `cargo test -p lakecat-sail --all-features`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Reusable read-restriction parser checks passed:
  `cargo fmt -p lakecat-security -p lakecat-service -- --check`,
  `cargo test -p lakecat-security read_restriction -- --nocapture`,
  `cargo test -p lakecat-security`,
  `cargo test -p lakecat-service table_scan_authorization_carries_policy_read_restriction -- --nocapture`, and
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Reusable read-restriction application checks passed:
  `cargo fmt -p lakecat-security -p lakecat-service -- --check`,
  `cargo test -p lakecat-security read_restriction -- --nocapture`,
  `cargo test -p lakecat-security`,
  `cargo test -p lakecat-service`,
  `cargo test -p lakecat-service effective_projection_cannot_widen_policy_columns -- --nocapture`, and
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Strict idempotency replay checks passed:
  `cargo fmt -p lakecat-store -p lakecat-service -p lakecat-sail -- --check`,
  `cargo test -p lakecat-service commit_replays_rest_idempotency_key -- --nocapture`,
  `cargo test -p lakecat-service --all-features commit_replays_rest_idempotency_key -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local turso_store_round_trips_namespaces_tables_and_idempotent_commits -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local`,
  `cargo test -p lakecat-service`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Audit-safe idempotency evidence focused checks passed:
  `cargo fmt -p lakecat-store -p lakecat-service -- --check`,
  `cargo test -p lakecat-service commit_replays_rest_idempotency_key -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local turso_store_round_trips_namespaces_tables_and_idempotent_commits -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local`,
  `cargo test -p lakecat-service`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Bounded metadata orphan cleanup focused checks passed:
  `cargo fmt -p lakecat-service -- --check`,
  `cargo test -p lakecat-service --features sail-local stale_commit_cleans_up_uncommitted_metadata_file -- --nocapture`,
  `cargo test -p lakecat-service --features sail-local stale_commit_requirement_returns_conflict_with_sail_local_engine -- --nocapture`,
  `cargo test -p lakecat-service --features sail-local`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- REST commit idempotency focused checks passed:
  `cargo fmt -p lakecat-service -- --check`,
  `cargo test -p lakecat-service commit_replays_rest_idempotency_key -- --nocapture`,
  `cargo test -p lakecat-service`,
  `cargo test -p lakecat-store --features turso-local`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- TypeSec delegate fallback focused checks passed:
  `cargo fmt -p lakecat-security -- --check`,
  `cargo test -p lakecat-security --features typesec-local delegates_to_typesec_fallback_policy_engine -- --nocapture`,
  `cargo test -p lakecat-security --features typesec-local delegates_authorization_to_typesec_policy_engine -- --nocapture`,
  `cargo test -p lakecat-security --features typesec-local`,
  `git diff --check`, and
  `cargo test --workspace --all-features`.
- Governed row-predicate focused checks passed:
  `cargo fmt -p lakecat-service -- --check`,
  `cargo test -p lakecat-service table_scan_authorization_carries_policy_read_restriction`,
  and
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`.
- Governed fetch-token reapplication focused checks passed:
  `cargo fmt -p lakecat-sail -p lakecat-service -- --check`,
  `cargo test -p lakecat-sail --features sail-local preserves_filter_context_and_prunes_loaded_file_bounds -- --nocapture`,
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`,
  `cargo test -p lakecat-service --all-features`,
  `cargo test -p lakecat-sail --all-features`,
  and `cargo test --workspace --all-features`.
- Governed read restriction focused checks passed:
  `cargo fmt -p lakecat-security -p lakecat-service -- --check`,
  `cargo test -p lakecat-security`,
  `cargo test -p lakecat-service table_scan_authorization_carries_policy_read_restriction`,
  `cargo test -p lakecat-service effective_projection_cannot_widen_policy_columns`,
  `cargo test -p lakecat-service --features sail-local scan_planning_applies_policy_column_restriction_before_sail -- --nocapture`,
  `cargo test -p lakecat-sail --all-features preserves_filter_context_and_prunes_loaded_file_bounds -- --nocapture`,
  `cargo test -p lakecat-service`,
  `cargo test -p lakecat-service --all-features`,
  `cargo test --workspace --all-features`,
  and `git diff --check`.
- Applied `ci/sail-patches/*.patch` with `git am` to a clean temporary Sail
  checkout at `7a34be78`.
- Temporary patched Sail checkout passed:
  `cargo test -p sail-catalog-iceberg planning -- --nocapture`.
- Current `lakehq/sail@ceab87693f8e37f50d855ba6cf479c3a169ccc95` accepted the
  patch series with the identity-configured `git am` command and passed:
  `cargo test -p sail-catalog-iceberg planning -- --nocapture`.
- A GitHub Actions-shaped temporary directory with sibling `sail/` and
  `lakecat/` paths successfully applied the patch series using the workflow's
  absolute `PATCH_DIR` shell block.
- Local workflow matrix commands passed before rerunning any cloud gate:
  `cargo test --workspace`,
  `cargo test -p lakecat-service --features sail-local`,
  `cargo test -p lakecat-security --features typesec-local`,
  `cargo test -p lakecat-service --features typesec-local`,
  `cargo test -p lakecat-graph --features grust-local`,
  `cargo test -p lakecat-graph --features grust-local grust_cypher_can_query_lakecat_catalog_projection_boundary -- --nocapture`,
  `cargo test -p lakecat-store --features turso-local`, and
  `cargo test --workspace --all-features`.
- QueryGraph OSI handoff focused checks passed:
  `cargo fmt -p lakecat-querygraph -- --check`,
  `cargo test -p lakecat-querygraph`,
  `cargo test -p lakecat-service querygraph_bootstrap_projects_catalog_tables`,
  and `git diff --check`.
- LakeCat focused Sail-local service test passed:
  `cargo test -p lakecat-service --features sail-local fetch_scan_tasks_exposes_iceberg_rest_plan_task_tokens -- --nocapture`.
- Manual GitHub Actions run `27720360961` after pushing TypeSec 0.8 reached the
  matrix tests. Passing rows: `grust-local graph`, `grust cypher boundary`,
  `typesec-local security`, and `turso-local store`. Failed rows now all report
  missing `protoc` from Sail's `sail-common-datafusion` build script.
- Manual GitHub Actions run `27720653125` after installing `protobuf-compiler`
  proved `protoc` is no longer missing. Passing rows: `grust cypher boundary`,
  `grust-local graph`, `typesec-local security`, and `turso-local store`.
  Failed rows now report missing Sail helper exports such as
  `LoadTableResult`, `load_table_result_to_status`,
  `completed_planning_with_id_result_from_values`, and
  `fetch_scan_tasks_result_from_values` from the cloud `lakehq/sail@main`
  checkout.
- `cargo fmt -p lakecat-api -p lakecat-cli -p lakecat-core -p lakecat-graph -p lakecat-lineage -p lakecat-querygraph -p lakecat-sail -p lakecat-security -p lakecat-service -p lakecat-store -- --check`
- `git diff --check`
- `cargo test -p lakecat-service --features typesec-local -- --nocapture`
- `cargo test -p lakecat-graph --features grust-local grust_cypher_can_query_lakecat_catalog_projection_boundary -- --nocapture`
- `git diff --check`
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

Rerun the manual GitHub Actions workflow after pushing the Sail patch bridge.
If the matrix is green, keep automatic push/PR triggers disabled until the Sail
helper commits can be checked out from a real upstream branch, then remove the
temporary `ci/sail-patches/` bridge.
